mod ffi;
use ffi::*;

use std::{ffi::{c_char, c_int, CStr}, collections::HashMap, mem};
use unicase::Ascii;



pub use ffi::llhttp_type_t as HttpType;

#[derive(Debug)]
pub struct Error {
    error: llhttp_errno_t,
    error_name: String,
}

impl From<llhttp_errno_t> for Error {
    fn from(value: llhttp_errno_t) -> Self {
        unsafe {
            Self {
                error: value,
                error_name: CStr::from_ptr(llhttp_errno_name(value))
                    .to_str()
                    .unwrap()
                    .to_string(),
            }
        }
    }
}

impl Error {
    pub fn error(&self) -> llhttp_errno_t {
        self.error
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }

    fn description(&self) -> &str {
        &self.error_name
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.source()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_name)
    }
}

#[repr(C)]
#[derive(Default)]
pub struct RequestParser {
    parser: ffi::llhttp_t,

    is_header_complete: bool,
    method: Option<String>,
    url: Option<String>,
    version: Option<String>,
    headers: HashMap<Ascii<String>, String>,

    method_buf: String,
    url_buf: String,
    version_buf: String,
    header_field_buf: String,
    header_value_buf: String,
    body_buf: Vec<u8>,

    // pub content: Option<String>,
}

unsafe impl Sync for RequestParser {}
unsafe impl Send for RequestParser {}

macro_rules! unsafe_cast_parser {
    ($value:expr) => {
        unsafe { &mut *($value as *mut RequestParser) }
    };
}

extern "C" fn request_on_headers_complete(parser: *mut ffi::llhttp_t) -> c_int {
    let parser = unsafe_cast_parser!(parser);
    parser.is_header_complete = true;
    0
}

extern "C" fn request_on_url(parser: *mut llhttp_t, data: *const c_char, len: usize) -> c_int {
    let parser = unsafe_cast_parser!(parser);
    
    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };

    parser.url_buf.push_str(std::str::from_utf8(vec).unwrap());
    0
}

extern "C" fn request_on_method(parser: *mut llhttp_t, data: *const c_char, len: usize) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };

    parser.method_buf.push_str(std::str::from_utf8(vec).unwrap());

    0
}

extern "C" fn request_on_version(parser: *mut llhttp_t, data: *const c_char, len: usize) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };
    
    parser.version_buf.push_str(std::str::from_utf8(vec).unwrap());

    0
}

extern "C" fn request_on_header_field(
    parser: *mut llhttp_t,
    data: *const c_char,
    len: usize,
) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };

    parser.header_field_buf.push_str(std::str::from_utf8(vec).unwrap());
    0
}

extern "C" fn request_on_header_value(parser: *mut ffi::llhttp_t, data: *const c_char, len: usize) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };

    parser.header_value_buf.push_str(std::str::from_utf8(vec).unwrap());

    0
}

extern "C" fn request_on_body(parser: *mut ffi::llhttp_t, data: *const c_char, len: usize) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let vec = unsafe { std::slice::from_raw_parts(data as *const u8, len) };

    parser.body_buf.extend_from_slice(vec);
    0
}

extern "C" fn request_on_header_value_complete(parser: *mut ffi::llhttp_t) -> c_int {
    let parser = unsafe_cast_parser!(parser);

    let field = mem::take(&mut parser.header_field_buf);
    let value = mem::take(&mut parser.header_value_buf);

    parser.headers.insert(Ascii::new(field), value);

    0
}

extern "C" fn request_on_method_complete(parser: *mut ffi::llhttp_t) -> c_int {
    let parser = unsafe_cast_parser!(parser);
    parser.method = Some(mem::take(&mut parser.method_buf));
    0
}

extern "C" fn request_on_url_complete(parser: *mut ffi::llhttp_t) -> c_int {
    let parser = unsafe_cast_parser!(parser);
    parser.url = Some(mem::take(&mut parser.url_buf));
    0
}

extern "C" fn request_on_version_complete(parser: *mut ffi::llhttp_t) -> c_int {
    let parser = unsafe_cast_parser!(parser);
    parser.version = Some(mem::take(&mut parser.version_buf));
    0
}

impl Drop for RequestParser {
    fn drop(&mut self) {
        let ptr = self.parser.settings;
        unsafe { llhttp_settings_free(ptr as *const _) };
    }
}

impl RequestParser {
    pub fn new() -> Self {
        unsafe {
            let settings = llhttp_settings_new();
            let mut parser = llhttp_t::new();

            ffi::llhttp_init(
                &mut parser as *mut llhttp_t,
                HttpType::HTTP_REQUEST,
                settings,
            );

            let settings = &mut *settings;
            settings.on_headers_complete = request_on_headers_complete;

            settings.on_url = request_on_url;
            settings.on_url_complete = request_on_url_complete;

            settings.on_version = request_on_version;
            settings.on_version_complete = request_on_version_complete;

            settings.on_method = request_on_method;
            settings.on_method_complete = request_on_method_complete;

            settings.on_header_field = request_on_header_field;

            settings.on_header_value = request_on_header_value;
            settings.on_header_value_complete = request_on_header_value_complete;

            settings.on_body = request_on_body;


            let mut ret = Self::default();
            ret.parser = parser;
            ret
        }
    }

    pub fn execute(&mut self, bytes: &[u8]) -> Result<(), Error> {

        let ptr = &mut self.parser as *mut llhttp_t;

        let res = unsafe { llhttp_execute(ptr, bytes.as_ptr() as *const _, bytes.len()) };

        if let llhttp_errno_t::HPE_OK = res {
            Ok(())
        } else {
            Err(Error::from(res))
        }
    }

    pub fn reset(&mut self) {
        unsafe {
            llhttp_reset(&mut self.parser as *mut _);
        }


        self.is_header_complete = false;
        self.method = None;
        self.url = None;
        self.version = None;
        self.headers.clear();
        self.method_buf.clear();
        self.url_buf.clear();
        self.version_buf.clear();
        self.header_field_buf.clear();
        self.header_value_buf.clear();
        self.body_buf.clear();
    }

    pub fn is_header_complete(&self) -> bool {
        self.is_header_complete
    }

    pub fn method(&self) -> Option<&str> {
        self.method.as_ref().map(|v| v.as_str())
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_ref().map(|v| v.as_str())
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_ref().map(|v| v.as_str())
    }

    pub fn headers(&self) -> &HashMap<Ascii<String>, String> {
        &self.headers
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        mem::take(&mut self.body_buf)
    }


    pub fn take_headers(&mut self) -> HashMap<Ascii<String>, String> {
        mem::take(&mut self.headers)
    }

}
