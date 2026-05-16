package com.tomlplus.test;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.tomlplus.Tomlplus;

import java.nio.file.Files;
import java.nio.file.Paths;

public final class Harness {
    public static void main(String[] args) throws Exception {
        String src = Files.readString(Paths.get(args[0]));
        try (var doc = Tomlplus.parse(src)) {
            ObjectMapper mapper = new ObjectMapper()
                    .enable(SerializationFeature.INDENT_OUTPUT)
                    .enable(SerializationFeature.ORDER_MAP_ENTRIES_BY_KEYS);
            System.out.println(mapper.writeValueAsString(doc.config()));
        }
    }
}
