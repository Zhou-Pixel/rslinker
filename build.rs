fn main() {
    let files = [
        "src/llhttp/http.c",
        "src/llhttp/api.c",
        "src/llhttp/llhttp.c",
        "src/llhttp/ffi.c"
    ];

    cc::Build::new().files(files).warnings(false).compile("llhttp");
}
