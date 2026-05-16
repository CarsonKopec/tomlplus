"""
Tests for the tomlplus library.
Run with: pytest tests/
"""

import os
import sys
import pytest


import tomlplus
from tomlplus import (
    loads, load_validated, loads_validated, dumps,
    validate, validate_all,
    ParseError, ValidationError, VariableError,
    TOMLPlusDocument, Annotation,
)


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# Helpers
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

def parse(src: str) -> TOMLPlusDocument:
    return loads(src)


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 1. Basic parsing
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestBasicParsing:

    def test_string(self):
        doc = parse('name = "hello"')
        assert doc["name"] == "hello"

    def test_integer(self):
        doc = parse("count = 42")
        assert doc["count"] == 42

    def test_negative_integer(self):
        doc = parse("offset = -7")
        assert doc["offset"] == -7

    def test_float(self):
        doc = parse("ratio = 3.14")
        assert abs(doc["ratio"] - 3.14) < 1e-9

    def test_bool_true(self):
        doc = parse("flag = true")
        assert doc["flag"] is True

    def test_bool_false(self):
        doc = parse("flag = false")
        assert doc["flag"] is False

    def test_null(self):
        doc = parse("nothing = null")
        assert doc["nothing"] is None

    def test_array(self):
        doc = parse('tags = ["a", "b", "c"]')
        assert doc["tags"] == ["a", "b", "c"]

    def test_empty_array(self):
        doc = parse("items = []")
        assert doc["items"] == []

    def test_section(self):
        doc = parse('[server]\nport = 9090')
        assert doc["server"]["port"] == 9090

    def test_multiple_sections(self):
        src = '[app]\nname = "x"\n[db]\nhost = "localhost"'
        doc = parse(src)
        assert doc["app"]["name"] == "x"
        assert doc["db"]["host"] == "localhost"

    def test_comments_stripped(self):
        doc = parse('# full comment\nport = 80  # inline')
        assert doc["port"] == 80

    def test_string_escapes(self):
        doc = parse(r'msg = "line1\nline2"')
        assert doc["msg"] == "line1\nline2"


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 2. Variables
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestVariables:

    def test_builtin_true(self):
        doc = parse("flag = $TRUE")
        assert doc["flag"] is True

    def test_builtin_false(self):
        doc = parse("flag = $FALSE")
        assert doc["flag"] is False

    def test_builtin_null(self):
        doc = parse("val = $NULL")
        assert doc["val"] is None

    def test_builtin_platform(self):
        doc = parse("p = $PLATFORM")
        assert isinstance(doc["p"], str)

    def test_builtin_now(self):
        doc = parse("ts = $NOW")
        assert isinstance(doc["ts"], str)
        assert "T" in doc["ts"]   # ISO format

    def test_user_var(self):
        src = '[vars]\nbase = "https://api.example.com"\n[service]\nurl = $base'
        doc = parse(src)
        assert doc["service"]["url"] == "https://api.example.com"

    def test_user_var_not_in_output(self):
        src = '[vars]\nsecret = "abc"\n[app]\nname = "test"'
        doc = parse(src)
        assert "vars" not in doc.config

    def test_env_var_unset_with_fallback(self):
        src = 'timeout = $ENV.TOTALLY_UNSET_VAR_XYZ ?? 30'
        doc = parse(src)
        assert doc["timeout"] == 30

    def test_env_var_set(self, monkeypatch):
        monkeypatch.setenv("TOMLPLUS_TEST_HOST", "my-host")
        doc = parse("host = $ENV.TOMLPLUS_TEST_HOST")
        assert doc["host"] == "my-host"

    def test_env_var_fallback_string(self, monkeypatch):
        monkeypatch.delenv("TOMLPLUS_TEST_MISSING", raising=False)
        doc = parse('host = $ENV.TOMLPLUS_TEST_MISSING ?? "fallback"')
        assert doc["host"] == "fallback"

    def test_undefined_variable_raises(self):
        with pytest.raises(VariableError):
            parse("x = $UNDEFINED_VAR_99")

    def test_string_concat(self):
        src = '[vars]\nbase = "hello"\n[app]\nmsg = $base + " world"'
        doc = parse(src)
        assert doc["app"]["msg"] == "hello world"

    def test_numeric_arithmetic(self):
        src = '[vars]\nx = 10\n[app]\ny = $x * 2'
        doc = parse(src)
        assert doc["app"]["y"] == 20

    def test_vars_property(self):
        src = '[vars]\nfoo = "bar"\n[app]\nname = "test"'
        doc = parse(src)
        assert doc.vars["foo"] == "bar"


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 3. Annotations
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestAnnotations:

    def test_annotation_stored_in_meta(self):
        doc = parse('@required\nkey = "value"')
        assert any(a.name == "required" for a in doc.annotations("key"))

    def test_type_annotation(self):
        doc = parse('@type: int\nport = 8080')
        anns = doc.annotations("port")
        ta = next(a for a in anns if a.name == "type")
        assert ta.arg == "int"

    def test_min_annotation(self):
        doc = parse('@min: 1\nport = 80')
        anns = doc.annotations("port")
        assert any(a.name == "min" and a.arg == 1 for a in anns)

    def test_enum_annotation(self):
        doc = parse('@enum: [debug, info, warn]\nlevel = "info"')
        anns = doc.annotations("level")
        ea = next(a for a in anns if a.name == "enum")
        assert "info" in ea.arg

    def test_tag_annotation(self):
        doc = parse('@tag: owner = "platform"\n[app]\nname = "test"')
        tags = doc.tags("app")
        assert tags.get("owner") == "platform"

    def test_deprecated_with_message(self):
        doc = parse('@deprecated("use new_key instead")\nold_key = "x"')
        anns = doc.annotations("old_key")
        da = next(a for a in anns if a.name == "deprecated")
        assert "new_key" in da.arg

    def test_multiple_annotations_stacked(self):
        src = '@required\n@type: string\n@minlen: 3\nusername = "bob"'
        doc = parse(src)
        names = {a.name for a in doc.annotations("username")}
        assert names == {"required", "type", "minlen"}

    def test_section_annotation(self):
        src = '@tag: owner = "team"\n[app]\nname = "x"'
        doc = parse(src)
        assert "owner" in doc.tags("app")

    def test_annotation_str(self):
        a = Annotation("type", "int")
        assert str(a) == "@type: int"

    def test_annotation_str_no_arg(self):
        a = Annotation("required")
        assert str(a) == "@required"

    def test_annotation_str_list_arg(self):
        a = Annotation("enum", ["a", "b", "c"])
        assert str(a) == "@enum: [a, b, c]"

    def test_has_annotation(self):
        doc = parse('@required\nkey = "v"')
        assert doc.has_annotation("key", "required")
        assert not doc.has_annotation("key", "deprecated")

    def test_required_keys(self):
        src = '@required\nkey1 = "v"\nkey2 = "v2"'
        doc = parse(src)
        assert "key1" in doc.required_keys()
        assert "key2" not in doc.required_keys()

    def test_deprecated_keys(self):
        src = '@deprecated("use new")\nold = "x"\nnew = "y"'
        doc = parse(src)
        deps = doc.deprecated_keys()
        assert any(k == "old" for k, _ in deps)

    def test_keys_with_tag(self):
        src = '@tag: owner = "billing"\n[payments]\nprovider = "stripe"'
        doc = parse(src)
        results = doc.keys_with_tag("owner")
        assert any(k == "payments" and v == "billing" for k, v in results)


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 4. Dictionaries
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestDictionaries:

    def test_inline_dict(self):
        doc = parse('colors = #{ primary = "#FF0000", secondary = "#00FF00" }#')
        assert doc["colors"]["primary"] == "#FF0000"

    def test_block_dict(self):
        src = '[server]\nheaders = #{\n  content-type = "application/json"\n}#'
        doc = parse(src)
        assert doc["server"]["headers"]["content-type"] == "application/json"

    def test_block_dict_with_annotations(self):
        src = (
            '[server]\n'
            'headers = #{\n'
            '  @required\n'
            '  content-type = "application/json"\n'
            '}#'
        )
        doc = parse(src)
        assert doc["server"]["headers"]["content-type"] == "application/json"
        anns = doc.annotations("server.headers.content-type")
        assert any(a.name == "required" for a in anns)


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 5. Validation
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestValidation:

    def test_required_passes(self):
        doc = parse('@required\nname = "alice"')
        validate(doc)   # should not raise

    def test_required_fails_empty_string(self):
        doc = parse('@required\nname = ""')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_required_fails_null(self):
        doc = parse('@required\nval = $NULL')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_type_int_passes(self):
        doc = parse('@type: int\nport = 8080')
        validate(doc)

    def test_type_int_fails_string(self):
        doc = parse('@type: int\nport = "oops"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_type_bool_passes(self):
        doc = parse('@type: bool\nflag = true')
        validate(doc)

    def test_type_bool_fails_int(self):
        doc = parse('@type: bool\nflag = 1')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_min_passes(self):
        doc = parse('@min: 1\nport = 80')
        validate(doc)

    def test_min_fails(self):
        doc = parse('@min: 100\nport = 80')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_max_passes(self):
        doc = parse('@max: 65535\nport = 8080')
        validate(doc)

    def test_max_fails(self):
        doc = parse('@max: 100\nport = 200')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_minlen_passes(self):
        doc = parse('@minlen: 3\nname = "alice"')
        validate(doc)

    def test_minlen_fails(self):
        doc = parse('@minlen: 10\nname = "hi"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_maxlen_passes(self):
        doc = parse('@maxlen: 10\nname = "alice"')
        validate(doc)

    def test_maxlen_fails(self):
        doc = parse('@maxlen: 3\nname = "toolongname"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_pattern_passes(self):
        doc = parse('@pattern: "[a-z]+"\nslug = "hello"')
        validate(doc)

    def test_pattern_fails(self):
        doc = parse('@pattern: "[a-z]+"\nslug = "Hello123"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_enum_passes(self):
        doc = parse('@enum: [debug, info, warn, error]\nlevel = "info"')
        validate(doc)

    def test_enum_fails(self):
        doc = parse('@enum: [debug, info, warn, error]\nlevel = "trace"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_positive_passes(self):
        doc = parse('@positive\nworkers = 4')
        validate(doc)

    def test_positive_fails_zero(self):
        doc = parse('@positive\nworkers = 0')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_positive_fails_negative(self):
        doc = parse('@positive\nworkers = -1')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_nonzero_passes(self):
        doc = parse('@nonzero\nval = 5')
        validate(doc)

    def test_nonzero_fails(self):
        doc = parse('@nonzero\nval = 0')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_nonempty_passes(self):
        doc = parse('@nonempty\nval = "hello"')
        validate(doc)

    def test_nonempty_fails_empty_string(self):
        doc = parse('@nonempty\nval = ""')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_type_url_passes(self):
        doc = parse('@type: url\nendpoint = "https://example.com"')
        validate(doc)

    def test_type_url_fails(self):
        doc = parse('@type: url\nendpoint = "not-a-url"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_type_email_passes(self):
        doc = parse('@type: email\naddr = "user@example.com"')
        validate(doc)

    def test_type_email_fails(self):
        doc = parse('@type: email\naddr = "notanemail"')
        with pytest.raises(ValidationError):
            validate(doc)

    def test_validate_all_returns_all_errors(self):
        src = '@min: 100\nport = 1\n@maxlen: 2\nname = "toolong"'
        doc = parse(src)
        errors = validate_all(doc)
        assert len(errors) == 2

    def test_validate_all_empty_on_valid(self):
        doc = parse('@min: 1\nport = 80')
        errors = validate_all(doc)
        assert errors == []

    def test_loads_validated_raises(self):
        with pytest.raises(ValidationError):
            loads_validated('@required\nkey = ""')

    def test_loads_validated_returns_doc(self):
        doc = loads_validated('@required\nkey = "value"')
        assert doc["key"] == "value"


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 6. TOMLPlusDocument API
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestDocument:

    def test_getitem(self):
        doc = parse('[app]\nname = "test"')
        assert doc["app"]["name"] == "test"

    def test_get_default(self):
        doc = parse('x = 1')
        assert doc.get("missing", 42) == 42

    def test_contains(self):
        doc = parse('x = 1')
        assert "x" in doc
        assert "y" not in doc

    def test_resolve_dotted(self):
        doc = parse('[db]\nhost = "localhost"')
        assert doc.resolve("db.host") == "localhost"

    def test_resolve_missing_returns_default(self):
        doc = parse('x = 1')
        assert doc.resolve("no.such.key", "default") == "default"

    def test_len(self):
        doc = parse('a = 1\nb = 2\nc = 3')
        assert len(doc) == 3

    def test_iter(self):
        doc = parse('a = 1\nb = 2')
        assert set(doc) == {"a", "b"}

    def test_repr(self):
        doc = parse('[server]\nport = 80')
        assert "server" in repr(doc)


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 7. Serializer (dumps)
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestDumps:

    def test_simple_key(self):
        result = dumps({"port": 8080})
        assert "port = 8080" in result

    def test_section(self):
        result = dumps({"server": {"port": 80}})
        assert "[server]" in result
        assert "port = 80" in result

    def test_string_value(self):
        result = dumps({"name": "hello"})
        assert '"hello"' in result

    def test_bool_value(self):
        result = dumps({"flag": True})
        assert "true" in result

    def test_null_value(self):
        result = dumps({"x": None})
        assert "null" in result

    def test_list_value(self):
        result = dumps({"tags": ["a", "b"]})
        assert '["a", "b"]' in result

    def test_roundtrip_preserves_values(self):
        src = '[app]\nname = "myapp"\nport = 9000\ndebug = false'
        doc = parse(src)
        serialized = dumps(doc)
        doc2 = parse(serialized)
        assert doc2["app"]["name"] == "myapp"
        assert doc2["app"]["port"] == 9000
        assert doc2["app"]["debug"] is False


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 8. Edge cases
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestEdgeCases:

    def test_empty_source(self):
        doc = parse("")
        assert doc.config == {}

    def test_only_comments(self):
        doc = parse("# just a comment\n# another")
        assert doc.config == {}

    def test_hyphenated_key(self):
        doc = parse('my-key = "value"')
        assert doc["my-key"] == "value"

    def test_underscore_key(self):
        doc = parse('my_key = "value"')
        assert doc["my_key"] == "value"

    def test_nested_fallback_chain(self):
        src = 'val = $ENV.NONEXISTENT_ABC ?? $ENV.ALSO_NONEXISTENT_XYZ ?? 99'
        doc = parse(src)
        assert doc["val"] == 99

    def test_annotation_is_metadata(self):
        assert Annotation("required").is_metadata
        assert Annotation("deprecated").is_metadata
        assert not Annotation("type", "int").is_metadata

    def test_annotation_is_type_hint(self):
        assert Annotation("type", "int").is_type_hint
        assert not Annotation("min", 1).is_type_hint

    def test_annotation_is_validation(self):
        assert Annotation("min", 1).is_validation
        assert Annotation("enum", ["a"]).is_validation
        assert not Annotation("required").is_validation

    def test_annotation_is_tag(self):
        assert Annotation("tag", "owner = me").is_tag
        assert not Annotation("required").is_tag

    def test_version(self):
        # Major bump: 1.0.0 (pure-Python) → 2.0.0 (Rust-backed wheel).
        assert tomlplus.__version__ == "2.0.0"


# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
# 9. Extended language features
# â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

class TestDottedSections:

    def test_two_level(self):
        doc = parse('[server.cors]\norigin = "*"')
        assert doc["server"]["cors"]["origin"] == "*"

    def test_three_level(self):
        doc = parse('[a.b.c]\nx = 1')
        assert doc["a"]["b"]["c"]["x"] == 1

    def test_sibling_sections_share_parent(self):
        src = '[server.cors]\norigin = "*"\n[server.tls]\nport = 443'
        doc = parse(src)
        assert doc["server"]["cors"]["origin"] == "*"
        assert doc["server"]["tls"]["port"] == 443

    def test_quoted_section_part(self):
        doc = parse('["my section".inner]\nx = 1')
        assert doc["my section"]["inner"]["x"] == 1

    def test_meta_uses_dotted_path(self):
        src = '[server.cors]\n@type: string\norigin = "*"'
        doc = parse(src)
        anns = doc.annotations("server.cors.origin")
        assert any(a.name == "type" for a in anns)


class TestQuotedKeys:

    def test_simple_quoted_key(self):
        doc = parse('"my key" = 1')
        assert doc["my key"] == 1

    def test_quoted_key_with_dot(self):
        doc = parse('"a.b" = "x"')
        assert doc["a.b"] == "x"

    def test_quoted_key_with_annotation(self):
        doc = parse('@type: int\n"odd-key" = 5')
        assert doc["odd-key"] == 5
        assert doc.has_annotation("odd-key", "type")


class TestExtendedNumbers:

    def test_hex_lower(self):
        doc = parse("flags = 0xff")
        assert doc["flags"] == 255

    def test_hex_upper(self):
        doc = parse("flags = 0XFF")
        assert doc["flags"] == 255

    def test_octal(self):
        doc = parse("perm = 0o755")
        assert doc["perm"] == 0o755

    def test_binary(self):
        doc = parse("mask = 0b10110")
        assert doc["mask"] == 0b10110

    def test_underscore_int(self):
        doc = parse("big = 1_000_000")
        assert doc["big"] == 1_000_000

    def test_underscore_float(self):
        doc = parse("pi = 3.14_159")
        assert abs(doc["pi"] - 3.14159) < 1e-9

    def test_scientific(self):
        doc = parse("e = 1.5e3")
        assert doc["e"] == 1500.0

    def test_negative_hex(self):
        doc = parse("x = -0x10")
        assert doc["x"] == -16


class TestMultilineArrays:

    def test_simple_multiline(self):
        src = 'tags = [\n  "a",\n  "b",\n  "c"\n]'
        doc = parse(src)
        assert doc["tags"] == ["a", "b", "c"]

    def test_trailing_comma(self):
        src = 'tags = [\n  "a",\n  "b",\n]'
        doc = parse(src)
        assert doc["tags"] == ["a", "b"]

    def test_nested_multiline(self):
        src = 'matrix = [\n  [1, 2],\n  [3, 4]\n]'
        doc = parse(src)
        assert doc["matrix"] == [[1, 2], [3, 4]]

    def test_multiline_inline_dict(self):
        src = 'opts = #{\n  a = 1,\n  b = 2\n}#'
        doc = parse(src)
        assert doc["opts"] == {"a": 1, "b": 2}

    def test_array_with_comments_inside(self):
        src = 'tags = [\n  "a",   # first\n  "b"\n]'
        doc = parse(src)
        assert doc["tags"] == ["a", "b"]


class TestExpressionsInCollections:

    def test_var_concat_in_array(self):
        src = (
            '[vars]\nbase = "https://api"\n'
            '[svc]\nendpoints = [$base + "/v1", $base + "/v2"]'
        )
        doc = parse(src)
        assert doc["svc"]["endpoints"] == ["https://api/v1", "https://api/v2"]

    def test_arithmetic_in_inline_dict(self):
        src = (
            '[vars]\nx = 10\n'
            '[svc]\nopts = #{ a = $x * 2, b = $x + 5 }#'
        )
        doc = parse(src)
        assert doc["svc"]["opts"] == {"a": 20, "b": 15}

    def test_fallback_in_array(self, monkeypatch):
        monkeypatch.delenv("UNSET_AAA", raising=False)
        src = 'vals = [$ENV.UNSET_AAA ?? "x", $ENV.UNSET_AAA ?? 99]'
        doc = parse(src)
        assert doc["vals"] == ["x", 99]


class TestRoundtripNew:

    def test_block_dict_roundtrip(self):
        src = (
            '[server]\n'
            'headers = #{\n'
            '  content-type = "application/json"\n'
            '  cache-control = "no-cache"\n'
            '}#\n'
        )
        doc = parse(src)
        out = dumps(doc)
        assert "#{" in out
        assert "}#" in out
        doc2 = parse(out)
        assert doc2["server"]["headers"]["content-type"] == "application/json"
        assert doc2["server"]["headers"]["cache-control"] == "no-cache"

    def test_inline_dict_roundtrip(self):
        doc = parse('opts = #{ a = 1, b = 2 }#')
        out = dumps(doc)
        doc2 = parse(out)
        assert doc2["opts"] == {"a": 1, "b": 2}

    def test_dotted_section_roundtrip(self):
        src = '[server.cors]\norigin = "*"\n'
        doc = parse(src)
        out = dumps(doc)
        # dumper currently flattens nested dicts back to whatever it can; the
        # important property is that the data round-trips even if structure
        # differs.
        doc2 = parse(out)
        assert doc2.resolve("server.cors.origin") == "*"


class TestSectionAnnotations:

    def test_section_tag_does_not_error(self):
        # Section-level @tag must not be validated against scalar constraints.
        doc = parse('@tag: owner = "team"\n[app]\nname = "x"')
        validate(doc)   # must not raise

    def test_leaf_only_annotation_skipped_on_section(self):
        # @type: int on a section dict should NOT trigger a type error.
        doc = parse('@type: int\n[server]\nport = 8080')
        validate(doc)

