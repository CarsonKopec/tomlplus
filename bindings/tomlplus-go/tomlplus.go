// Package tomlplus wraps the TOML+ C library (`tomlplus_ffi`) via cgo.
//
// At build time, cgo needs:
//   - the directory containing `tomlplus.h`               (CGO_CFLAGS / -I)
//   - the directory containing `tomlplus_ffi.{dll,so,dylib}` (CGO_LDFLAGS / -L)
//
// Typical setup (from the workspace root):
//
//   set CGO_CFLAGS=-I"%CD%\crates\tomlplus-ffi\include"
//   set CGO_LDFLAGS=-L"%CD%\target\release"
//   go test ./...
package tomlplus

/*
#include <stdlib.h>
#include <tomlplus.h>

// On Windows the FFI library is `tomlplus_ffi.dll` and Rust's MSVC-flavoured
// static archive `tomlplus_ffi.lib` is binary-incompatible with the MinGW ld
// that backs cgo, so we link directly against the DLL (a GNU-ld feature).
#cgo windows LDFLAGS: -l:tomlplus_ffi.dll
#cgo linux   LDFLAGS: -ltomlplus_ffi -ldl
#cgo darwin  LDFLAGS: -ltomlplus_ffi
*/
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"runtime"
	"strings"
	"sync"
	"unsafe"
)

// ── Document handle ──────────────────────────────────────────────────────────

// Document owns the underlying parser handle. Close to release it eagerly,
// or rely on the GC finalizer.
type Document struct {
	handle *C.TomlplusDoc
	once   sync.Once
}

// Close releases the underlying handle. Safe to call multiple times.
func (d *Document) Close() {
	d.once.Do(func() {
		if d.handle != nil {
			C.tomlplus_free(d.handle)
			d.handle = nil
		}
	})
}

// ── Top-level API ────────────────────────────────────────────────────────────

// Parse parses a TOML+ source string.
func Parse(source string) (*Document, error) {
	csource := C.CString(source)
	defer C.free(unsafe.Pointer(csource))

	h := C.tomlplus_parse(csource)
	if h == nil {
		return nil, errors.New(lastError())
	}
	doc := &Document{handle: h}
	runtime.SetFinalizer(doc, (*Document).Close)
	return doc, nil
}

// Load reads a .tomlp file from disk and parses it.
func Load(path string) (*Document, error) {
	data, err := readFile(path)
	if err != nil {
		return nil, err
	}
	return Parse(data)
}

// Validate runs the annotation validator. Returns the first error, if any.
func Validate(d *Document) error {
	errs, err := ValidateAll(d)
	if err != nil {
		return err
	}
	for _, e := range errs {
		if e.Severity == "error" {
			return fmt.Errorf("%s", e.Message)
		}
	}
	return nil
}

// ValidationError is a single annotation-driven diagnostic.
type ValidationError struct {
	Message  string `json:"message"`
	Severity string `json:"severity"`
	Span     struct {
		Start int `json:"start"`
		End   int `json:"end"`
	} `json:"span"`
}

// ValidateAll returns every annotation-validator diagnostic.
func ValidateAll(d *Document) ([]ValidationError, error) {
	p := C.tomlplus_validate(d.handle)
	if p == nil {
		return nil, errors.New(lastError())
	}
	defer C.tomlplus_free_string(p)
	var out []ValidationError
	if err := json.Unmarshal([]byte(C.GoString(p)), &out); err != nil {
		return nil, err
	}
	return out, nil
}

// Dumps re-serialises the document back to TOML+ text.
func Dumps(d *Document) (string, error) {
	p := C.tomlplus_dumps(d.handle)
	if p == nil {
		return "", errors.New(lastError())
	}
	defer C.tomlplus_free_string(p)
	return C.GoString(p), nil
}

// Version returns the underlying library's CARGO_PKG_VERSION.
func Version() string {
	return C.GoString(C.tomlplus_version())
}

// ── Document accessors ──────────────────────────────────────────────────────

// Config returns the parsed config tree as a plain map.
func (d *Document) Config() (map[string]interface{}, error) {
	return readJSONObject(C.tomlplus_to_json(d.handle))
}

// Vars returns the resolved [vars] section.
func (d *Document) Vars() (map[string]interface{}, error) {
	return readJSONObject(C.tomlplus_vars_json(d.handle))
}

// Meta returns annotation metadata keyed by dotted path.
func (d *Document) Meta() (map[string][]Annotation, error) {
	p := C.tomlplus_meta_json(d.handle)
	if p == nil {
		return nil, errors.New(lastError())
	}
	defer C.tomlplus_free_string(p)
	var out map[string][]Annotation
	if err := json.Unmarshal([]byte(C.GoString(p)), &out); err != nil {
		return nil, err
	}
	return out, nil
}

// Annotation is one `@…` line attached to a key.
type Annotation struct {
	Name string      `json:"name"`
	Arg  interface{} `json:"arg"`
}

// Resolve walks a dotted path (e.g. "server.port") into the config tree.
// Returns nil if any segment is missing.
func (d *Document) Resolve(path string) (interface{}, error) {
	cfg, err := d.Config()
	if err != nil {
		return nil, err
	}
	parts := splitDotted(path)
	var node interface{} = cfg
	for _, part := range parts {
		m, ok := node.(map[string]interface{})
		if !ok {
			return nil, nil
		}
		node, ok = m[part]
		if !ok {
			return nil, nil
		}
	}
	return node, nil
}

// ── Helpers ─────────────────────────────────────────────────────────────────

func readJSONObject(p *C.char) (map[string]interface{}, error) {
	if p == nil {
		return nil, errors.New(lastError())
	}
	defer C.tomlplus_free_string(p)
	var out map[string]interface{}
	if err := json.Unmarshal([]byte(C.GoString(p)), &out); err != nil {
		return nil, err
	}
	return out, nil
}

func lastError() string {
	p := C.tomlplus_last_error()
	if p == nil {
		return "unknown error"
	}
	return C.GoString(p)
}

func splitDotted(s string) []string {
	return strings.Split(s, ".")
}

// readFile is split out so cgo doesn't drag os/ioutil into the FFI surface.
func readFile(path string) (string, error) {
	b, err := osReadFile(path)
	if err != nil {
		return "", err
	}
	return string(b), nil
}
