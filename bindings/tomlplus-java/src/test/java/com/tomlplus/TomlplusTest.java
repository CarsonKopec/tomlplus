package com.tomlplus;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class TomlplusTest {

    @Test
    void parseAndResolve() {
        try (var doc = Tomlplus.parse("""
                [server]
                @type: int
                @min: 1
                @max: 65535
                port = 8080
                """)) {
            assertEquals(8080, ((Number) doc.resolve("server.port")).intValue());
            assertTrue(doc.hasAnnotation("server.port", "type"));
        }
    }

    @Test
    void validateThrowsOnBadValue() {
        try (var doc = Tomlplus.parse("@min: 100\nport = 1")) {
            assertThrows(Tomlplus.TomlplusException.class, () -> Tomlplus.validate(doc));
        }
    }

    @Test
    void dumpsRoundTrip() {
        try (var doc  = Tomlplus.parse("[a]\nx = 1");
             var doc2 = Tomlplus.parse(Tomlplus.dumps(doc))) {
            assertEquals(1, ((Number) doc2.resolve("a.x")).intValue());
        }
    }

    @Test
    void varsAndExpressions() {
        try (var doc = Tomlplus.parse("""
                [vars]
                base = "https://api"
                [svc]
                url = $base + "/v1"
                """)) {
            assertEquals("https://api/v1", doc.resolve("svc.url"));
        }
    }
}
