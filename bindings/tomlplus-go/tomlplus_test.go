package tomlplus

import "testing"

func TestParseAndResolve(t *testing.T) {
	doc, err := Parse(`
[server]
@type: int
@min: 1
@max: 65535
port = 8080
`)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	defer doc.Close()

	v, err := doc.Resolve("server.port")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	if got, want := v.(float64), 8080.0; got != want {
		t.Fatalf("server.port = %v, want %v", got, want)
	}
}

func TestValidateFailsOnBadValue(t *testing.T) {
	doc, _ := Parse(`@min: 100
port = 1`)
	defer doc.Close()
	if err := Validate(doc); err == nil {
		t.Fatal("expected validate to fail, got nil")
	}
}

func TestDumpsRoundTrip(t *testing.T) {
	doc, _ := Parse(`[a]
x = 1`)
	defer doc.Close()
	s, err := Dumps(doc)
	if err != nil {
		t.Fatalf("dumps: %v", err)
	}
	doc2, err := Parse(s)
	if err != nil {
		t.Fatalf("re-parse: %v", err)
	}
	defer doc2.Close()
}
