"""A tiny assembler for the SM64 decomp's sound-player sequence (sound/sequences/00_sound_player.s + include/seq_macros.inc).

The Python twin of native/byor-rs/src/seq_asm.rs (same subset, same errors): the in-process layer (inproc.py) uses it in
place of `clang -x assembler-with-cpp`, which iOS does not have.  Only what those two files use is implemented:

  cpp:     #include "file", #if (defined(X), !, &&, ||, integers, parentheses), #ifdef, #ifndef, #else, #endif, // and /* */
  GNU as:  .section .rodata, .align/.balign N[, fill], .byte, .set NAME, expr / NAME = expr, labels, .macro/.endm with
           \\param, param=default and \\@, .if/.else/.endif, integer expressions with GNU (llvm-mc) precedence

Anything else is an error, so a decomp change outside this subset fails loudly instead of producing different bytes.

  seq_asm.py DECOMP_DIR OUT.bin     assemble the US sound player
"""
import re
import sys
from pathlib import Path


class AsmError(Exception):
    pass


IDENT = re.compile(r'[A-Za-z_.$][A-Za-z0-9_.$]*$')
WORD = re.compile(r'[A-Za-z0-9_.$]+')


def sound_player(decomp):
    decomp = Path(decomp)
    return assemble_file(decomp / 'sound/sequences/00_sound_player.s', [decomp / 'include'], ['VERSION_US'])


def assemble_file(path, include_dirs, defines):
    path = Path(path)

    def load(name, frm):
        for d in [Path(frm).parent, *map(Path, include_dirs)]:
            if (d / name).is_file():
                return str(d / name), (d / name).read_text()
        raise AsmError(f'{frm}: #include "{name}" not found')
    return _assemble(preprocess(path.read_text(), str(path), list(defines), load))


def assemble_str(text, includes=None, defines=()):
    includes = includes or {}

    def load(name, frm):
        if name not in includes:
            raise AsmError(f'{frm}: #include "{name}" not found')
        return name, includes[name]
    return _assemble(preprocess(text, '<input>', list(defines), load))


# ---------------------------------------------------------------------------------------------------------------------
# C preprocessor subset

def strip_comments(text):
    out, i, n = [], 0, len(text)
    start, in_str = 0, False
    while i < n:
        c = text[i]
        if in_str:
            in_str = c not in '"\n'
            i += 1
        elif c == '"':
            in_str = True
            i += 1
        elif text.startswith('//', i):
            out.append(text[start:i])
            j = text.find('\n', i)
            i = start = n if j < 0 else j
        elif text.startswith('/*', i):
            out.append(text[start:i] + ' ')
            j = text.find('*/', i + 2)
            end = n if j < 0 else j + 2
            out.append('\n' * text.count('\n', i, end))
            i = start = end
        else:
            i += 1
    out.append(text[start:])
    return ''.join(out)


def preprocess(text, file, defines, load, depth=0):
    if depth > 16:
        raise AsmError(f'{file}: #include nested too deeply')
    out, stack = [], []  # stack: [active, taken, parent active]
    for n, raw in enumerate(strip_comments(text).split('\n'), 1):
        at = f'{file}:{n}'
        t = raw.strip()
        on = not stack or stack[-1][0]
        if t.startswith('#'):
            parts = t[1:].split(None, 1)
            word, rest = (parts[0], parts[1].strip() if len(parts) > 1 else '') if parts else ('', '')
            if word in ('if', 'ifdef', 'ifndef'):
                if not on:
                    v = False
                elif word == 'if':
                    try:
                        v = cpp_expr(rest, defines) != 0
                    except AsmError as e:
                        raise AsmError(f'{at}: {e}')
                else:
                    if not IDENT.match(rest):
                        raise AsmError(f'{at}: #{word} needs one name')
                    v = (rest in defines) == (word == 'ifdef')
                stack.append([on and v, v, on])
            elif word == 'else':
                if not stack:
                    raise AsmError(f'{at}: #else without #if')
                stack[-1][0] = stack[-1][2] and not stack[-1][1]
                stack[-1][1] = True
            elif word == 'endif':
                if not stack:
                    raise AsmError(f'{at}: #endif without #if')
                stack.pop()
            elif not on:
                pass
            elif word == 'include':
                if not (len(rest) > 1 and rest[0] == rest[-1] == '"'):
                    raise AsmError(f'{at}: only #include "file" is supported')
                path, body = load(rest[1:-1], file)
                out += preprocess(body, path, defines, load, depth + 1)
            else:
                raise AsmError(f'{at}: unsupported preprocessor directive #{word}')
            continue
        if not on or not t:
            continue
        used = [d for d in defines if d in WORD.findall(t)]
        if used:
            raise AsmError(f'{at}: {used[0]} used outside a preprocessor directive')
        out.append((t, at))
    if stack:
        raise AsmError(f'{file}: unterminated #if')
    return out


def cpp_expr(s, defines):
    t = re.sub(r'\bdefined\s*(?:\(\s*([A-Za-z_]\w*)\s*\)|([A-Za-z_]\w*))',
               lambda m: ' 1 ' if (m.group(1) or m.group(2)) in defines else ' 0 ', s)
    bad = [w for w in WORD.findall(t) if not w[0].isdigit()]
    if bad:
        raise AsmError(f'unsupported #if operand {bad[0]}')
    return evaluate(parse_expr(t), lambda _: None)


# ---------------------------------------------------------------------------------------------------------------------
# expressions: ('num', v) | ('sym', name) | ('un', op, a) | ('bin', op, a, b)

OPS = ['||', '&&', '==', '!=', '<>', '<=', '>=', '<<', '>>', '<', '>', '+', '-', '|', '&', '^', '*', '/', '%', '~', '!', '(', ')']
PREC = {'||': 1, '&&': 2, '==': 3, '!=': 3, '<>': 3, '<': 3, '<=': 3, '>': 3, '>=': 3, '+': 4, '-': 4,
        '|': 5, '&': 5, '^': 5, '*': 6, '/': 6, '%': 6, '<<': 6, '>>': 6}


def parse_int(s):
    try:
        if s[:2].lower() == '0x':
            return int(s[2:], 16)
        if s[:2].lower() == '0b':
            return int(s[2:], 2)
        if len(s) > 1 and s[0] == '0':
            return int(s[1:], 8)
        return int(s, 10)
    except ValueError:
        raise AsmError(f'bad number {s}') from None


def lex(s):
    toks, i = [], 0
    while i < len(s):
        c = s[i]
        if c.isspace():
            i += 1
        elif c.isdigit():
            m = re.compile(r'[A-Za-z0-9]+').match(s, i)
            toks.append(('num', parse_int(m.group())))
            i = m.end()
        elif re.match(r'[A-Za-z0-9_.$]', c):
            m = WORD.match(s, i)
            toks.append(('sym', m.group()))
            i = m.end()
        else:
            op = next((o for o in OPS if s.startswith(o, i)), None)
            if op is None:
                raise AsmError(f"unexpected '{c}' in expression {s}")
            toks.append(('op', op))
            i += len(op)
    return toks


def parse_expr(s):
    toks = lex(s)
    pos = [0]

    def peek():
        return toks[pos[0]] if pos[0] < len(toks) else None

    def unary():
        t = peek()
        if t is None:
            raise AsmError(f'unexpected end in expression `{s}`')
        pos[0] += 1
        if t[0] in ('num', 'sym'):
            return t
        if t[1] == '(':
            e = binary(1)
            if peek() != ('op', ')'):
                raise AsmError(f'missing ) in expression `{s}`')
            pos[0] += 1
            return e
        if t[1] in '-+~!':
            return ('un', t[1], unary())
        raise AsmError(f'unexpected {t[1]} in expression `{s}`')

    def binary(lo):
        lhs = unary()
        while True:
            t = peek()
            if not t or t[0] != 'op' or PREC.get(t[1], 0) < lo:
                return lhs
            pos[0] += 1
            lhs = ('bin', t[1], lhs, binary(PREC[t[1]] + 1))

    e = binary(1)
    if pos[0] != len(toks):
        raise AsmError(f'trailing tokens in expression `{s}`')
    return e


def evaluate(e, sym):
    k = e[0]
    if k == 'num':
        return e[1]
    if k == 'sym':
        v = sym(e[1])
        if v is None:
            raise AsmError(f'undefined symbol {e[1]}')
        return v
    if k == 'un':
        a = evaluate(e[2], sym)
        return {'-': -a, '+': a, '~': ~a, '!': int(a == 0)}[e[1]]
    op, a, b = e[1], evaluate(e[2], sym), evaluate(e[3], sym)
    if op in ('/', '%') and b == 0:
        raise AsmError('division by zero')
    if op in ('<<', '>>') and not 0 <= b < 64:
        raise AsmError(f'shift by {b}')
    if op == '>>' and a < 0:
        raise AsmError(f'right shift of a negative value ({a})')
    if op == '/':
        return int(a / b)
    if op == '%':
        return a - int(a / b) * b
    return {'||': lambda: int(bool(a) or bool(b)), '&&': lambda: int(bool(a) and bool(b)), '==': lambda: int(a == b),
            '!=': lambda: int(a != b), '<>': lambda: int(a != b), '<': lambda: int(a < b), '<=': lambda: int(a <= b),
            '>': lambda: int(a > b), '>=': lambda: int(a >= b), '+': lambda: a + b, '-': lambda: a - b, '|': lambda: a | b,
            '&': lambda: a & b, '^': lambda: a ^ b, '*': lambda: a * b, '<<': lambda: a << b, '>>': lambda: a >> b}[op]()


def resolve(e, sym):
    if e[0] == 'sym':
        v = sym(e[1])
        return e if v is None else ('num', v)
    if e[0] == 'un':
        return ('un', e[1], resolve(e[2], sym))
    if e[0] == 'bin':
        return ('bin', e[1], resolve(e[2], sym), resolve(e[3], sym))
    return e


def split_args(s):
    out, cur, depth = [], '', 0
    for c in s:
        if c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
        elif c == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
            continue
        cur += c
    if cur.strip() or out:
        out.append(cur.strip())
    return out


OPERATOR = '+-~/*%=|^&!<>'


def split_macro_args(s):
    """llvm-mc: commas separate, and outside parentheses so does whitespace unless it sits next to an operator."""
    c = s.strip()
    out, cur, depth, i = [], '', 0, 0
    while i < len(c):
        ch = c[i]
        if ch == '(':
            depth += 1
        elif ch == ')':
            depth -= 1
        elif ch == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
            i += 1
            continue
        elif ch.isspace() and depth == 0:
            while i < len(c) and c[i].isspace():
                i += 1
            nxt = c[i] if i < len(c) else None
            joined = (nxt is not None and nxt in OPERATOR) or (cur.rstrip()[-1:] in tuple(OPERATOR))
            if joined or not cur.strip() or nxt in (None, ','):
                cur += ' '
            else:
                out.append(cur.strip())
                cur = ''
            continue
        cur += ch
        i += 1
    if cur.strip() or out:
        out.append(cur.strip())
    return out


# ---------------------------------------------------------------------------------------------------------------------
# assembler

class _Asm:
    def __init__(self):
        self.macros, self.labels, self.sets = {}, {}, {}
        self.bytes, self.conds = [], []  # conds: [active, parent active]
        self.in_rodata, self.count, self.depth = False, 0, 0

    def active(self):
        return not self.conds or self.conds[-1][0]

    def lookup(self, s):
        return self.sets.get(s, self.labels.get(s))

    def value(self, s, at):
        try:
            return evaluate(parse_expr(s), self.lookup)
        except AsmError as e:
            raise AsmError(f'{at}: {e}') from None

    def need_rodata(self, at):
        if not self.in_rodata:
            raise AsmError(f'{at}: data outside .section .rodata')

    def statement(self, lines, i):
        text, at = lines[i]
        if ';' in text:
            raise AsmError(f"{at}: ';' statement separators are not supported")
        s = text.strip()
        parts = s.split(None, 1)
        word, rest = (parts[0], parts[1].strip() if len(parts) > 1 else '') if parts else ('', '')
        if word == '.if':
            on = self.active()
            self.conds.append([on and self.value(rest, at) != 0, on])
            return i + 1
        if word == '.else':
            if not self.conds:
                raise AsmError(f'{at}: .else without .if')
            self.conds[-1][0] = self.conds[-1][1] and not self.conds[-1][0]
            return i + 1
        if word == '.endif':
            if not self.conds:
                raise AsmError(f'{at}: .endif without .if')
            self.conds.pop()
            return i + 1
        if not self.active():
            if word == '.macro':
                raise AsmError(f'{at}: .macro inside .if is not supported')
            return i + 1
        if word == '.macro':
            return self.define(lines, i)
        while ':' in s:
            name, _, s = s.partition(':')
            name, s = name.strip(), s.strip()
            if not IDENT.match(name):
                raise AsmError(f'{at}: bad label `{name}`')
            if name in self.labels or name in self.sets:
                raise AsmError(f'{at}: symbol {name} defined twice')
            if not self.in_rodata:
                raise AsmError(f'{at}: label {name} outside .section .rodata')
            self.labels[name] = len(self.bytes)
        if not s:
            return i + 1
        parts = s.split(None, 1)
        word, rest = parts[0], (parts[1].strip() if len(parts) > 1 else '')
        name, eq, value = s.partition('=')
        if eq and IDENT.match(name.strip()) and not value.startswith('='):
            self.set(name.strip(), value, at)
            return i + 1
        if word == '.section':
            if rest != '.rodata':
                raise AsmError(f'{at}: only .section .rodata is supported, not {rest}')
            self.in_rodata = True
        elif word in ('.align', '.balign'):
            args = split_args(rest)
            if not 1 <= len(args) <= 2:
                raise AsmError(f'{at}: {word} ALIGN[, FILL] (no max-skip) is supported')
            n = self.value(args[0], at)
            if n < 0 or (n > 1 and n & (n - 1)):
                raise AsmError(f'{at}: bad alignment {n}')
            fill = self.value(args[1], at) if len(args) > 1 else 0
            if not -128 <= fill <= 255:
                raise AsmError(f'{at}: bad fill value {fill}')
            self.need_rodata(at)
            while n > 1 and len(self.bytes) % n:
                self.bytes.append((('num', fill), at))
        elif word == '.byte':
            self.need_rodata(at)
            for a in split_args(rest):
                if not a:
                    raise AsmError(f'{at}: empty .byte operand')
                try:
                    self.bytes.append((resolve(parse_expr(a), self.lookup), at))
                except AsmError as e:
                    raise AsmError(f'{at}: {e}') from None
        elif word in ('.set', '.equ'):
            name, comma, value = rest.partition(',')
            if not comma:
                raise AsmError(f'{at}: {word} NAME, VALUE')
            self.set(name.strip(), value, at)
        elif word == '.endm':
            raise AsmError(f'{at}: .endm without .macro')
        elif word in self.macros:
            self.expand(word, rest, at)
        else:
            raise AsmError(f'{at}: unsupported directive or unknown macro `{word}`')
        return i + 1

    def set(self, name, value, at):
        if not IDENT.match(name) or name in self.labels:
            raise AsmError(f'{at}: cannot .set {name}')
        self.sets[name] = self.value(value, at)

    def define(self, lines, i):
        at = lines[i][1]
        head = lines[i][0].strip()[6:].strip()
        m = re.match(r'([^\s,]+)[\s,]*(.*)$', head)
        if not m or not IDENT.match(m.group(1)):
            raise AsmError(f'{at}: bad macro name `{head}`')
        name, params = m.group(1), []
        for p in re.split(r'[\s,]+', m.group(2)):
            if not p:
                continue
            n, eq, d = p.partition('=')
            if not IDENT.match(n) or '.' in n:
                raise AsmError(f'{at}: bad macro parameter `{p}`')
            params.append((n, d if eq else None))
        j, body = i + 1, []
        while True:
            if j >= len(lines):
                raise AsmError(f'{at}: .macro {name} without .endm')
            w = (lines[j][0].split() or [''])[0]
            if w == '.endm':
                break
            if w == '.macro':
                raise AsmError(f'{lines[j][1]}: nested .macro')
            body.append(lines[j])
            j += 1
        if name in self.macros:
            raise AsmError(f'{at}: macro {name} defined twice')
        self.macros[name] = (params, body)
        return j + 1

    def expand(self, name, args, at):
        if self.depth > 32:
            raise AsmError(f'{at}: macro recursion too deep')
        params, body = self.macros[name]
        args = split_macro_args(args)
        if len(args) > len(params):
            raise AsmError(f'{at}: {name} takes {len(params)} arguments, got {len(args)}')
        vals = {}
        for k, (p, d) in enumerate(params):
            v = args[k] if k < len(args) and args[k] else d
            if v is None:
                raise AsmError(f'{at}: {name}: missing argument {p}')
            vals[p] = v
        n = str(self.count)
        self.count += 1

        def sub(m):
            if m.group(1) == '@':
                return n
            if m.group(1) == '()':
                return ''
            if m.group(1) not in vals:
                raise AsmError(f'unknown macro parameter \\{m.group(1)}')
            return vals[m.group(1)]
        expanded = []
        for text, lat in body:
            try:
                expanded.append((re.sub(r'\\(@|\(\)|[A-Za-z0-9_]*)', sub, text), f'{lat} (from {at})'))
            except AsmError as e:
                raise AsmError(f'{lat}: {e}') from None
        depth = len(self.conds)
        self.depth += 1
        i = 0
        while i < len(expanded):
            i = self.statement(expanded, i)
        self.depth -= 1
        if len(self.conds) != depth:
            raise AsmError(f'{at}: unbalanced .if in macro {name}')


def _assemble(lines):
    a = _Asm()
    i = 0
    while i < len(lines):
        i = a.statement(lines, i)
    if a.conds:
        raise AsmError('unterminated .if')
    out = bytearray()
    for e, at in a.bytes:
        try:
            v = evaluate(e, a.labels.get)
        except AsmError as err:
            raise AsmError(f'{at}: {err}') from None
        if not -128 <= v <= 255:  # like llvm-mc: a .byte must fit in 8 bits, signed or unsigned
            raise AsmError(f'{at}: .byte value {v} out of range')
        out.append(v & 0xff)
    return bytes(out)


if __name__ == '__main__':
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    Path(sys.argv[2]).write_bytes(sound_player(sys.argv[1]))
