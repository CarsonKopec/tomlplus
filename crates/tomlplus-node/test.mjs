import { test } from "node:test";
import assert from "node:assert/strict";
import { parse, validate, validateAll, dumps } from "./index.js";

test("parse + resolve", () => {
    const doc = parse(`[server]
@type: int
@min: 1
@max: 65535
port = 8080`);
    assert.equal(doc.resolve("server.port"), 8080);
    assert.equal(doc.hasAnnotation("server.port", "type"), true);
});

test("annotations come through", () => {
    const doc = parse(`@required\nname = "x"`);
    const anns = doc.annotations("name");
    assert.ok(anns.find(a => a.name === "required"));
});

test("validate throws on bad value", () => {
    const doc = parse(`@min: 100\nport = 1`);
    assert.throws(() => validate(doc));
});

test("dumps round-trips", () => {
    const doc = parse(`[a]\nx = 1`);
    const out = dumps(doc);
    const doc2 = parse(out);
    assert.equal(doc2.resolve("a.x"), 1);
});

test("vars + builtins", () => {
    const doc = parse(`[vars]\nbase = "https://x"\n[s]\nu = $base + "/v1"`);
    assert.equal(doc.resolve("s.u"), "https://x/v1");
});
