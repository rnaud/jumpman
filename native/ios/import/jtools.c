// `_jtools`: the decomp's command-line tools as in-process functions, for platforms without fork/exec (iOS).
//
//   _jtools.run(name, [argv...]) -> (exit_code, stdout_bytes, stderr_bytes)
//
// `name` is one of mio0, n64graphics, skyconv, aifc_decode, aiff_extract_codebook, vadpcm_enc.  The tool sources are
// the unmodified decomp files, compiled with jt_compat.h (see there).  Tools keep file-scope state and are not
// re-entrant, so calls are serialised (they run with the GIL held, which also serialises them).
#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include <setjmp.h>
#include <string.h>
#include <unistd.h>
#include <getopt.h>
#include <dlfcn.h>
#include <mach-o/getsect.h>
#include <mach-o/dyld.h>
#include "jt_compat.h"

#undef exit
FILE *jt_out, *jt_err;
static jmp_buf jt_jb;
static int jt_active;

void jt_exit(int rc) {
    if (jt_active) longjmp(jt_jb, rc + 1);
    _exit(rc);
}

extern int mio0_main(int, char **);
extern int n64graphics_main(int, char **);
extern int skyconv_main(int, char **);
extern int aifc_decode_main(int, char **);
extern int aiff_extract_codebook_main(int, char **);
extern int vadpcm_enc_main(int, char **);

// Each tool's file-scope data is linked into __DATA,jtd_<tag> / jtb_<tag> (see the jt_*.c wrappers).  The state of a
// tool before its first run is saved and put back before every later run, so every call starts like a new process.
static struct tool {
    const char *name; int (*fn)(int, char **); const char *tag;
    void *saved_d, *saved_b; unsigned long len_d, len_b; int saved;
} TOOLS[] = {
    {"mio0", mio0_main, "mio0"}, {"n64graphics", n64graphics_main, "n64g"}, {"skyconv", skyconv_main, "sky"},
    {"aifc_decode", aifc_decode_main, "aifc"}, {"aiff_extract_codebook", aiff_extract_codebook_main, "cb"},
    {"vadpcm_enc", vadpcm_enc_main, "vad"},
};

static void *section(const char *prefix, const char *tag, unsigned long *len) {
    Dl_info info;
    if (!dladdr((void *)jt_exit, &info)) return NULL;
    char name[32];
    snprintf(name, sizeof name, "%s_%s", prefix, tag);
    return getsectiondata((const struct mach_header_64 *)info.dli_fbase, "__DATA", name, len);
}

static void reset_state(struct tool *t) {
    unsigned long ld = 0, lb = 0;
    void *d = section("jtd", t->tag, &ld), *b = section("jtb", t->tag, &lb);
    if (!t->saved) {
        t->saved = 1;
        t->len_d = ld; t->len_b = lb;
        if (d && ld) { t->saved_d = malloc(ld); memcpy(t->saved_d, d, ld); }
        if (b && lb) { t->saved_b = malloc(lb); memcpy(t->saved_b, b, lb); }
    } else {
        if (d && t->saved_d) memcpy(d, t->saved_d, ld);
        if (b && t->saved_b) memcpy(b, t->saved_b, lb);
    }
}

static PyObject *jt_run(PyObject *self, PyObject *args) {
    const char *name;
    PyObject *list;
    if (!PyArg_ParseTuple(args, "sO!", &name, &PyList_Type, &list)) return NULL;
    struct tool *tool = NULL;
    for (size_t i = 0; i < sizeof TOOLS / sizeof *TOOLS; i++)
        if (!strcmp(TOOLS[i].name, name)) tool = &TOOLS[i];
    if (!tool) return PyErr_Format(PyExc_ValueError, "unknown tool %s", name);
    int (*fn)(int, char **) = tool->fn;
    reset_state(tool);
    Py_ssize_t n = PyList_GET_SIZE(list);
    char **argv = calloc((size_t)n + 2, sizeof *argv);
    argv[0] = strdup(name);
    for (Py_ssize_t i = 0; i < n; i++) {
        PyObject *s = PyList_GET_ITEM(list, i);
        const char *c = PyUnicode_AsUTF8(s);
        if (!c) { for (Py_ssize_t j = 0; j <= i; j++) free(argv[j]); free(argv); return NULL; }
        argv[i + 1] = strdup(c);
    }
    char *out = NULL, *err = NULL;
    size_t out_n = 0, err_n = 0;
    jt_out = open_memstream(&out, &out_n);
    jt_err = open_memstream(&err, &err_n);
    optind = 1;
#ifdef __APPLE__
    optreset = 1;
#endif
    int rc;
    jt_active = 1;
    int jumped = setjmp(jt_jb);
    if (jumped) rc = jumped - 1;
    else rc = fn((int)n + 1, argv);
    jt_active = 0;
    fflush(jt_out); fflush(jt_err);
    fclose(jt_out); fclose(jt_err);
    jt_out = jt_err = NULL;
    for (Py_ssize_t j = 0; j <= n; j++) free(argv[j]);
    free(argv);
    PyObject *r = Py_BuildValue("(iy#y#)", rc, out ? out : "", (Py_ssize_t)out_n, err ? err : "", (Py_ssize_t)err_n);
    free(out); free(err);
    return r;
}

static PyMethodDef METHODS[] = {{"run", jt_run, METH_VARARGS, "run(name, argv) -> (rc, stdout, stderr)"}, {NULL, NULL, 0, NULL}};
static struct PyModuleDef MODULE = {PyModuleDef_HEAD_INIT, "_jtools", "decomp tools, in-process", -1, METHODS};
PyMODINIT_FUNC PyInit__jtools(void) { return PyModule_Create(&MODULE); }
