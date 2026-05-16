package com.tomlplus;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

import java.util.List;
import java.util.Map;

/**
 * TOML+ Java bindings — thin wrapper over the {@code tomlplus_ffi} C library
 * loaded via JNA. The JVM needs the shared library on its native search path
 * ({@code -Djna.library.path=...} or the OS loader).
 */
public final class Tomlplus {
    private Tomlplus() {}

    private interface Lib extends Library {
        Lib INSTANCE = Native.load("tomlplus_ffi", Lib.class);

        Pointer tomlplus_parse(String source);
        Pointer tomlplus_to_json(Pointer doc);
        Pointer tomlplus_meta_json(Pointer doc);
        Pointer tomlplus_vars_json(Pointer doc);
        Pointer tomlplus_validate(Pointer doc);
        Pointer tomlplus_dumps(Pointer doc);
        void    tomlplus_free(Pointer doc);
        void    tomlplus_free_string(Pointer s);
        String  tomlplus_last_error();
        String  tomlplus_version();
    }

    private static final ObjectMapper JSON = new ObjectMapper();

    public static String version() {
        return Lib.INSTANCE.tomlplus_version();
    }

    /** Parse a TOML+ source string. */
    public static Document parse(String source) {
        Pointer h = Lib.INSTANCE.tomlplus_parse(source);
        if (h == null) {
            throw new TomlplusException(Lib.INSTANCE.tomlplus_last_error());
        }
        return new Document(h);
    }

    /** Validate and throw on the first failing constraint. */
    public static void validate(Document doc) {
        for (Map<String, Object> err : doc.validateAll()) {
            if ("error".equals(err.get("severity"))) {
                throw new TomlplusException((String) err.get("message"));
            }
        }
    }

    /** Re-serialise to TOML+ text. */
    public static String dumps(Document doc) {
        Pointer p = Lib.INSTANCE.tomlplus_dumps(doc.handle());
        return takeString(p);
    }

    // ── Parsed document handle ───────────────────────────────────────────────

    public static final class Document implements AutoCloseable {
        private Pointer handle;
        private final Object lock = new Object();

        Document(Pointer h) { this.handle = h; }

        Pointer handle() {
            if (handle == null) throw new IllegalStateException("Document is closed");
            return handle;
        }

        /** Whole config tree as a {@code Map<String, Object>}. */
        public Map<String, Object> config() {
            return readJsonMap(Lib.INSTANCE.tomlplus_to_json(handle()));
        }

        /** Resolved {@code [vars]} entries. */
        public Map<String, Object> vars() {
            return readJsonMap(Lib.INSTANCE.tomlplus_vars_json(handle()));
        }

        /** Annotation metadata, keyed by dotted path. */
        @SuppressWarnings("unchecked")
        public Map<String, List<Annotation>> meta() {
            String json = takeString(Lib.INSTANCE.tomlplus_meta_json(handle()));
            try {
                Map<String, List<Map<String, Object>>> raw = JSON.readValue(json, Map.class);
                java.util.LinkedHashMap<String, List<Annotation>> out = new java.util.LinkedHashMap<>();
                for (var e : raw.entrySet()) {
                    var anns = new java.util.ArrayList<Annotation>(e.getValue().size());
                    for (var a : e.getValue()) {
                        anns.add(new Annotation((String) a.get("name"), a.get("arg")));
                    }
                    out.put(e.getKey(), anns);
                }
                return out;
            } catch (Exception ex) {
                throw new TomlplusException("meta JSON parse failed: " + ex.getMessage());
            }
        }

        /** Walk a dotted path; returns {@code null} if any segment is missing. */
        public Object resolve(String path) {
            Object node = config();
            for (String part : path.split("\\.")) {
                if (!(node instanceof Map)) return null;
                node = ((Map<?, ?>) node).get(part);
                if (node == null) return null;
            }
            return node;
        }

        public boolean hasAnnotation(String path, String name) {
            var anns = meta().get(path);
            if (anns == null) return false;
            return anns.stream().anyMatch(a -> name.equals(a.name()));
        }

        @SuppressWarnings("unchecked")
        public List<Map<String, Object>> validateAll() {
            String json = takeString(Lib.INSTANCE.tomlplus_validate(handle()));
            try {
                return JSON.readValue(json, List.class);
            } catch (Exception ex) {
                throw new TomlplusException("validate JSON parse failed: " + ex.getMessage());
            }
        }

        @Override
        public void close() {
            synchronized (lock) {
                if (handle != null) {
                    Lib.INSTANCE.tomlplus_free(handle);
                    handle = null;
                }
            }
        }
    }

    /** A `@name: arg` annotation. */
    public record Annotation(String name, Object arg) {}

    /** Thrown for parse failures, validation errors, and FFI errors. */
    public static final class TomlplusException extends RuntimeException {
        public TomlplusException(String msg) { super(msg); }
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    @SuppressWarnings("unchecked")
    private static Map<String, Object> readJsonMap(Pointer p) {
        String s = takeString(p);
        try {
            return JSON.readValue(s, Map.class);
        } catch (Exception e) {
            throw new TomlplusException("JSON parse failed: " + e.getMessage());
        }
    }

    private static String takeString(Pointer p) {
        if (p == null) {
            throw new TomlplusException(Lib.INSTANCE.tomlplus_last_error());
        }
        try {
            return p.getString(0, "UTF-8");
        } finally {
            Lib.INSTANCE.tomlplus_free_string(p);
        }
    }
}
