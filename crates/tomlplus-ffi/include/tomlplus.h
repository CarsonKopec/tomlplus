/* tomlplus.h — C ABI for the TOML+ language core.
 *
 * Every output string is owned by the library; callers must release with
 * tomlplus_free_string(). Every opaque handle is released with tomlplus_free().
 * Errors are stored thread-locally; check tomlplus_last_error() after any call
 * that returns NULL.
 */
#ifndef TOMLPLUS_H
#define TOMLPLUS_H

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle returned by tomlplus_parse. */
typedef struct TomlplusDoc TomlplusDoc;

/* Parse a NUL-terminated UTF-8 TOML+ source string. Returns NULL on a fatal
 * error; check tomlplus_last_error() for the message. */
TomlplusDoc* tomlplus_parse(const char* source);

/* Serialise the parsed config tree as JSON. Caller must free with
 * tomlplus_free_string. */
char* tomlplus_to_json(const TomlplusDoc* doc);

/* Serialise annotation metadata as { "key.path": [{name, arg}, ...] }. */
char* tomlplus_meta_json(const TomlplusDoc* doc);

/* Serialise [vars] as a JSON object. */
char* tomlplus_vars_json(const TomlplusDoc* doc);

/* Run the validator. Returns a JSON array (possibly empty) of error objects:
 *   [{ "message": "...", "severity": "error|warning|...", "span": {start, end} }] */
char* tomlplus_validate(const TomlplusDoc* doc);

/* Re-serialise the document back to TOML+ text. */
char* tomlplus_dumps(const TomlplusDoc* doc);

/* Free a string returned by this library. Safe with NULL. */
void  tomlplus_free_string(char* s);

/* Free a document handle. Safe with NULL. */
void  tomlplus_free(TomlplusDoc* doc);

/* Last error message on the calling thread, or NULL. Owned by the library. */
const char* tomlplus_last_error(void);

/* Library version (CARGO_PKG_VERSION). Owned by the library. */
const char* tomlplus_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* TOMLPLUS_H */
