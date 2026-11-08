"""In-process execution layer for platforms without fork/exec (iOS).

The exporters, the decomp's `extract_assets.py` and `assemble_sound.py` are written for a desktop: they spawn
`python3`, `make`, `cc`, `clang` and the decomp's C tools.  On iOS none of that exists, so `install()` replaces the
`subprocess` entry points with an in-process dispatcher:

  python3 / sys.executable script.py ...   -> runpy in this interpreter (cwd, argv, stdout captured)
  make ...                                 -> no-op (the tools are linked into the app)
  mio0 / n64graphics / skyconv / aifc_decode -> `_jtools.run` (the unmodified decomp C sources, compiled into the app)
  cc <aiff_extract_codebook.c | vadpcm_enc.c ...> -o X -> writes a stub X; running X calls `_jtools`
  cc ids.c -o ids                          -> `ids` prints the SOUND_* constants, evaluated by `mini_cpp` below
  clang -x assembler-with-cpp 00_sound_player.s -o sfx.o -> an ELF stub whose .rodata is the precomputed bytes of that
                                              file (scripts/music/sm64-us-sound-player.bin), checked against the
                                              source's sha256 so a changed decomp file cannot silently use stale bytes

The same module also runs on macOS (`JUMPMAN_INPROC=1`, with a `_jtools` built for the host Python) so the iOS path is
testable, and byte-compared with the subprocess path, without a device.
"""
import contextlib, gc, hashlib, io, json, os, re, runpy, struct, sys, threading, traceback
from pathlib import Path

LOCK = threading.RLock()
CANCEL = threading.Event()
STUB = b'#!jtool '


class Cancelled(BaseException):
    pass


def _jt():
    import _jtools
    return _jtools


# ----------------------------------------------------------------------------------------------------------------
# running a Python script in-process

class LineWriter(io.TextIOBase):
    """stdout replacement: line callback + cancellation point."""

    def __init__(self, on_line):
        self.on_line, self.buf = on_line, ''

    def writable(self):
        return True

    def write(self, s):
        if CANCEL.is_set():
            raise Cancelled()
        self.buf += s
        while '\n' in self.buf:
            line, self.buf = self.buf.split('\n', 1)
            self.on_line(line)
        return len(s)

    def flush(self):
        pass


def run_script(argv, cwd=None, out=None):
    """Execute `argv[1]` (a .py path) as `__main__` with sys.argv=argv[1:]; returns the exit status."""
    cwd = Path(cwd) if cwd else Path.cwd()
    saved = (sys.argv, sys.path[:], os.getcwd(), sys.stdout, sys.stderr, set(sys.modules))
    script = (cwd / str(argv[1])).resolve()
    rc = 0
    try:
        os.chdir(cwd)
        sys.argv = [str(script)] + [str(a) for a in argv[2:]]
        sys.path.insert(0, str(script.parent))
        if out is not None:
            sys.stdout = sys.stderr = out
        try:
            runpy.run_path(str(script), run_name='__main__')
        except SystemExit as e:
            rc = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
            if isinstance(e.code, str):
                print(e.code, file=sys.stderr)
        except Cancelled:
            raise
        except Exception:
            traceback.print_exc()
            rc = 1
    finally:
        sys.argv, sys.path[:] = saved[0], saved[1]
        os.chdir(saved[2])
        sys.stdout, sys.stderr = saved[3], saved[4]
        # scripts import each other (mk64_courses, ...): forget them so the next stage's copies are re-read
        root = script.parent.parent
        for name in set(sys.modules) - saved[5]:
            f = getattr(sys.modules[name], '__file__', None)
            if f and str(f).startswith(str(root)):
                del sys.modules[name]
        gc.collect()
    return rc


def run_step(cmd, cwd, on_line):
    """pipeline.run() in-process: `cmd` is [sys.executable, script, args...]; raises RuntimeError like the subprocess path."""
    tail = []

    def line(l):
        tail.append(l.rstrip())
        del tail[:-12]
        on_line(l)

    w = LineWriter(line)
    rc = run_script(cmd, cwd, w)
    if rc:
        raise RuntimeError(f'{" ".join(map(str, cmd[:3]))} failed ({rc}):\n' + '\n'.join(tail))


# ----------------------------------------------------------------------------------------------------------------
# mini C preprocessor: enough of sounds.h to evaluate SOUND_* constants (what render-music's ids.c prints)

def _strip_comments(t):
    t = re.sub(r'/\*.*?\*/', ' ', t, flags=re.S)
    return re.sub(r'//[^\n]*', '', t)


class MiniCpp:
    def __init__(self, defines, include_dirs):
        self.macros = {d: ([], '1', False) for d in defines}
        self.inc = [Path(p) for p in include_dirs]
        self.done = set()

    def load(self, path):
        text = _strip_comments(Path(path).read_text()).replace('\\\n', ' ')
        stack = []  # (taking, any_taken)

        def on():
            return all(t for t, _ in stack)

        for raw in text.split('\n'):
            line = raw.strip()
            if not line.startswith('#'):
                continue
            m = re.match(r'#\s*(\w+)\s*(.*)', line)
            d, rest = m.group(1), m.group(2).strip()
            if d in ('ifdef', 'ifndef'):
                v = (rest in self.macros) == (d == 'ifdef')
                stack.append((v, v))
            elif d == 'if':
                v = bool(self.eval_cond(rest)) if on() else False
                stack.append((v, v))
            elif d == 'elif':
                t, a = stack.pop()
                v = (not a) and bool(self.eval_cond(rest))
                stack.append((v, a or v))
            elif d == 'else':
                t, a = stack.pop()
                stack.append((not a, True))
            elif d == 'endif':
                stack.pop()
            elif not on():
                continue
            elif d == 'define':
                m = re.match(r'(\w+)(\(([^)]*)\))?\s*(.*)', rest)
                name, params, body = m.group(1), m.group(3), m.group(4)
                self.macros[name] = ([p.strip() for p in params.split(',')] if params is not None and params.strip() else [],
                                     body, params is not None)
            elif d == 'undef':
                self.macros.pop(rest, None)
            elif d == 'include':
                m = re.match(r'["<]([^">]+)[">]', rest)
                if m:
                    for base in [Path(path).parent] + self.inc:
                        f = base / m.group(1)
                        if f.is_file() and f not in self.done:
                            self.done.add(f)
                            self.load(f)
                            break

    def eval_cond(self, expr):
        expr = re.sub(r'defined\s*\(\s*(\w+)\s*\)', lambda m: '1' if m.group(1) in self.macros else '0', expr)
        expr = re.sub(r'defined\s+(\w+)', lambda m: '1' if m.group(1) in self.macros else '0', expr)
        expr = expr.replace('&&', ' and ').replace('||', ' or ')
        expr = re.sub(r'!(?!=)', ' not ', expr)
        return self.value(expr)

    def expand(self, s, depth=0):
        if depth > 40:
            raise ValueError('macro recursion')
        out, i = [], 0
        while i < len(s):
            m = re.compile(r'[A-Za-z_]\w*').match(s, i)
            if not m:
                out.append(s[i]); i += 1; continue
            name = m.group(0); i = m.end()
            if name not in self.macros:
                out.append(name); continue
            params, body, fn = self.macros[name]
            if not fn:
                out.append(self.expand(body, depth + 1)); continue
            j = i
            while j < len(s) and s[j] == ' ':
                j += 1
            if j >= len(s) or s[j] != '(':
                out.append(name); continue
            level, args, cur, j = 1, [], '', j + 1
            while j < len(s):
                c = s[j]
                if c == '(':
                    level += 1
                elif c == ')':
                    level -= 1
                    if level == 0:
                        break
                if c == ',' and level == 1:
                    args.append(cur); cur = ''
                else:
                    cur += c
                j += 1
            args.append(cur)
            i = j + 1
            args = [self.expand(a.strip(), depth + 1) for a in args]
            sub = re.sub(r'\b[A-Za-z_]\w*\b', lambda mm: args[params.index(mm.group(0))] if mm.group(0) in params else mm.group(0), body)
            out.append(self.expand(sub, depth + 1))
        return ''.join(out)

    def value(self, expr):
        e = self.expand(expr)
        e = re.sub(r'\(\s*(?:u32|unsigned(?: int)?|s32|int)\s*\)', '', e)
        e = re.sub(r'\b(0[xX][0-9a-fA-F]+|\d+)[uUlL]+\b', r'\1', e)
        return eval(e, {'__builtins__': {}}, {})  # only integer expressions from the decomp's headers


def sound_ids(source_text, defines, include_dirs):
    """What render-music.py's generated ids.c prints: `printf("%u\\n", (unsigned)SOUND_X);` per line."""
    cpp = MiniCpp(defines, include_dirs)
    hdr = re.search(r'#include\s+"(sounds\.h)"', source_text)
    for base in include_dirs:
        if (Path(base) / hdr.group(1)).is_file():
            cpp.load(Path(base) / hdr.group(1))
    lines = []
    for name in re.findall(r'printf\("%u\\n", \(unsigned\)(\w+)\);', source_text):
        lines.append(str(cpp.value(name) & 0xFFFFFFFF))
    return ('\n'.join(lines) + '\n').encode()


# ----------------------------------------------------------------------------------------------------------------
# ELF stub for the precomputed sound-player sequence

def make_elf_rodata(blob):
    names = b'\0.rodata\0.shstrtab\0'
    data_off = 64
    str_off = data_off + len(blob)
    sh_off = (str_off + len(names) + 7) & ~7
    ehdr = b'\x7fELF\x02\x01\x01' + b'\0' * 9 + struct.pack('<HHIQQQIHHHHHH', 1, 0x3e, 1, 0, 0, sh_off, 0, 64, 0, 0, 64, 3, 2)
    sh = lambda n, t, off, size: struct.pack('<IIQQQQIIQQ', n, t, 2, 0, off, size, 0, 0, 1, 0)
    sections = b'\0' * 64 + sh(1, 1, data_off, len(blob)) + sh(9, 3, str_off, len(names))
    return ehdr + blob + names + b'\0' * (sh_off - str_off - len(names)) + sections


def _precomputed_sound_player():
    for base in (Path(__file__).resolve().parent,):
        f = base / 'sm64-us-sound-player.bin'
        if f.is_file():
            return json.loads(f.with_suffix('.json').read_text()), f.read_bytes()
    raise RuntimeError('missing sm64-us-sound-player.bin (run byor/gen-precomputed.py on a Mac)')


def sound_player_digest(src, include_dirs):
    """sha256 over the .s file and the macro include it pulls in: ties the precomputed bytes to the decomp sources."""
    macros = next((Path(d) / 'seq_macros.inc' for d in include_dirs if (Path(d) / 'seq_macros.inc').is_file()), None)
    return hashlib.sha256(Path(src).read_bytes() + b'\0' + (macros.read_bytes() if macros else b'')).hexdigest()


def _assemble_sound_player(src, out, cwd, include_dirs):
    meta, blob = _precomputed_sound_player()
    if sound_player_digest(Path(cwd, src), [Path(cwd, d) for d in include_dirs]) != meta['source_sha256']:
        raise RuntimeError('00_sound_player.s changed since sm64-us-sound-player.bin was generated; '
                           'run byor/gen-precomputed.py on a Mac')
    Path(out).write_bytes(make_elf_rodata(blob))


# ----------------------------------------------------------------------------------------------------------------
# subprocess replacement

TOOL_NAMES = {'mio0', 'n64graphics', 'skyconv', 'aifc_decode', 'aiff_extract_codebook', 'vadpcm_enc'}


class Done:
    def __init__(self, args, rc, out, err):
        self.args, self.returncode, self.stdout, self.stderr = args, rc, out, err

    def check_returncode(self):
        if self.returncode:
            import subprocess
            raise subprocess.CalledProcessError(self.returncode, self.args, self.stdout, self.stderr)


def _exec(argv, cwd):
    argv = [str(a) for a in argv]
    name = os.path.basename(argv[0])
    with LOCK:
        old = os.getcwd()
        if cwd:
            os.chdir(cwd)
        try:
            here = os.getcwd()
            if name == 'make':
                return 0, b'', b''
            if name in ('python3', 'python') or argv[0] == sys.executable:
                buf = io.StringIO()
                rc = run_script(argv, here, buf)
                return rc, buf.getvalue().encode(), b''
            if name in TOOL_NAMES:
                return _jt().run(name, argv[1:])
            if name == 'cc' or name == 'clang':
                return _compile(argv, here)
            f = Path(argv[0])
            if f.is_file() and f.open('rb').read(len(STUB)) == STUB:
                kind, _, payload = f.read_bytes()[len(STUB):].partition(b'\n')
                kind = kind.decode()
                if kind == 'ids':
                    return 0, sound_ids(*json.loads(payload)), b''
                return _jt().run(kind, argv[1:])
            return 127, b'', f'inproc: cannot run {argv[0]}\n'.encode()
        finally:
            os.chdir(old)


def _compile(argv, cwd):
    out = argv[argv.index('-o') + 1] if '-o' in argv else None
    srcs = [a for a in argv[1:] if a.endswith(('.c', '.s'))]
    if not srcs or not out:
        return 1, b'', b'inproc: unsupported compiler invocation\n'
    first = os.path.basename(srcs[0])
    if first == '00_sound_player.s':
        try:
            _assemble_sound_player(srcs[0], out, cwd, [argv[i + 1] for i, a in enumerate(argv) if a == '-I'])
        except RuntimeError as e:
            return 1, b'', str(e).encode()
        return 0, b'', b''
    if first == 'ids.c':
        defs = [a[2:] for a in argv if a.startswith('-D')]
        incs = [argv[i + 1] for i, a in enumerate(argv) if a == '-I']
        payload = json.dumps([Path(srcs[0]).read_text(), defs, incs]).encode()
        Path(out).write_bytes(STUB + b'ids\n' + payload)
        return 0, b'', b''
    tool = first[:-2]
    if tool in TOOL_NAMES:
        Path(out).write_bytes(STUB + tool.encode() + b'\n')
        return 0, b'', b''
    return 1, b'', f'inproc: unknown source {first}\n'.encode()


def install():
    """Replace subprocess.run/check_call/check_output (Popen is only used by pipeline.run, which is routed separately)."""
    import subprocess

    def run(args, *, check=False, stdout=None, stderr=None, cwd=None, text=None, input=None, capture_output=False, env=None, **kw):
        rc, out, err = _exec(args, cwd)
        if rc and err and os.environ.get('JT_DEBUG'):
            open(os.environ['JT_DEBUG'], 'ab').write(err)
        if rc and err:
            sys.stderr.write(err.decode(errors='replace'))
        if hasattr(stdout, 'write'):
            stdout.write(out); stdout.flush()
            out = None
        elif stdout != subprocess.PIPE and not capture_output:
            out = None
        if text and out is not None:
            out = out.decode()
        r = Done(args, rc, out, err if (capture_output or stderr == subprocess.PIPE) else None)
        if check:
            r.check_returncode()
        return r

    def check_call(args, **kw):
        run(args, check=True, **kw)
        return 0

    def check_output(args, **kw):
        return run(args, check=True, stdout=subprocess.PIPE, **kw).stdout

    subprocess.run, subprocess.check_call, subprocess.check_output = run, check_call, check_output
