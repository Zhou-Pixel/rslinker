use std::ffi::{c_char, c_int, c_void};

#[allow(non_camel_case_types)]
type llhttp_data_cb = extern "C" fn(*mut llhttp_t, *const c_char, usize) -> c_int;

#[allow(non_camel_case_types)]
type llhttp_cb = extern "C" fn(*mut llhttp_t) -> c_int;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct llhttp_t {
    pub _index: i32,
    pub _span_pos0: *const c_void,
    pub _span_cb0: *const c_void,
    pub error: i32,
    pub reason: *const c_char,
    pub error_pos: *const c_char,
    pub data: *const c_void,
    pub _current: *const c_void,
    pub content_length: u64,
    pub _type: u8,
    pub method: u8,
    pub http_major: u8,
    pub http_minor: u8,
    pub header_state: u8,
    pub lenient_flags: u16,
    pub upgrade: u8,
    pub finish: u8,
    pub flags: u16,
    pub status_code: u16,
    pub initial_message_completed: u8,
    pub settings: *const c_void,
}

impl Default for llhttp_t {
    fn default() -> Self {
        Self::new()
    }
}

impl llhttp_t {
    pub fn new() -> Self {
        unsafe {
            std::mem::zeroed()
        }
    }
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct llhttp_settings_t {
    /* Possible return values 0, -1, `HPE_PAUSED` */
    pub on_message_begin: llhttp_data_cb,

    pub on_url: llhttp_data_cb,
    pub on_status: llhttp_data_cb,
    pub on_method: llhttp_data_cb,
    pub on_version: llhttp_data_cb,
    pub on_header_field: llhttp_data_cb,
    pub on_header_value: llhttp_data_cb,
    pub on_chunk_extension_name: llhttp_data_cb,
    pub on_chunk_extension_value: llhttp_data_cb,

    /* Possible return values:
    * 0  - Proceed normally
    * 1  - Assume that request/response has no body, and proceed to parsing the
    *      next message
    * 2  - Assume absence of body (as above) and make `llhttp_execute()` return
    *      `HPE_PAUSED_UPGRADE`
    * -1 - Error
    * `HPE_PAUSED`
    */
    pub on_headers_complete: llhttp_cb,

    /* Possible return values 0, -1, HPE_USER */
    pub on_body: llhttp_data_cb,

    /* Possible return values 0, -1, `HPE_PAUSED` */
    pub on_message_complete: llhttp_cb,
    pub on_url_complete: llhttp_cb,
    pub on_status_complete: llhttp_cb,
    pub on_method_complete: llhttp_cb,
    pub on_version_complete: llhttp_cb,
    pub on_header_field_complete: llhttp_cb,
    pub on_header_value_complete: llhttp_cb,
    pub on_chunk_extension_name_complete: llhttp_cb,
    pub on_chunk_extension_value_complete: llhttp_cb,
    pub on_chunk_header: llhttp_cb,
    pub on_chunk_complete: llhttp_cb,
    pub on_reset: llhttp_cb,
}


// impl llhttp_settings_t {
//     pub fn new() -> Self {
//         unsafe {
//             // I know what I am doing
//             #[allow(invalid_value)]
//             std::mem::zeroed()
//         }
//     }
// }

#[repr(C)]
#[allow(non_camel_case_types)]
pub enum llhttp_type_t {
    HTTP_BOTH = 0,
    HTTP_REQUEST = 1,
    HTTP_RESPONSE = 2
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum llhttp_errno_t {
    HPE_OK = 0,
    HPE_INTERNAL = 1,
    HPE_STRICT = 2,
    HPE_CR_EXPECTED = 25,
    HPE_LF_EXPECTED = 3,
    HPE_UNEXPECTED_CONTENT_LENGTH = 4,
    HPE_UNEXPECTED_SPACE = 30,
    HPE_CLOSED_CONNECTION = 5,
    HPE_INVALID_METHOD = 6,
    HPE_INVALID_URL = 7,
    HPE_INVALID_CONSTANT = 8,
    HPE_INVALID_VERSION = 9,
    HPE_INVALID_HEADER_TOKEN = 10,
    HPE_INVALID_CONTENT_LENGTH = 11,
    HPE_INVALID_CHUNK_SIZE = 12,
    HPE_INVALID_STATUS = 13,
    HPE_INVALID_EOF_STATE = 14,
    HPE_INVALID_TRANSFER_ENCODING = 15,
    HPE_CB_MESSAGE_BEGIN = 16,
    HPE_CB_HEADERS_COMPLETE = 17,
    HPE_CB_MESSAGE_COMPLETE = 18,
    HPE_CB_CHUNK_HEADER = 19,
    HPE_CB_CHUNK_COMPLETE = 20,
    HPE_PAUSED = 21,
    HPE_PAUSED_UPGRADE = 22,
    HPE_PAUSED_H2_UPGRADE = 23,
    HPE_USER = 24,
    HPE_CB_URL_COMPLETE = 26,
    HPE_CB_STATUS_COMPLETE = 27,
    HPE_CB_METHOD_COMPLETE = 32,
    HPE_CB_VERSION_COMPLETE = 33,
    HPE_CB_HEADER_FIELD_COMPLETE = 28,
    HPE_CB_HEADER_VALUE_COMPLETE = 29,
    HPE_CB_CHUNK_EXTENSION_NAME_COMPLETE = 34,
    HPE_CB_CHUNK_EXTENSION_VALUE_COMPLETE = 35,
    HPE_CB_RESET = 31,
}

extern "C" {
    pub fn llhttp_execute(parser: *mut llhttp_t, data: *const c_char, len: usize) -> llhttp_errno_t;
    // pub fn llhttp_get_type(parser: *const llhttp_t) -> u8;
    // pub fn llhttp_get_http_major(parser: *const llhttp_t) -> u8;

    // pub fn llhttp_get_http_minor(parser: *const llhttp_t) -> u8;
    // pub fn llhttp_get_method(parser: *const llhttp_t) -> u8;
    // pub fn llhttp_get_status_code(parser: *const llhttp_t) -> c_int;
    // pub fn llhttp_get_upgrade(parser: *const llhttp_t) -> u8;

    pub fn llhttp_reset(parser: *mut llhttp_t);

    // pub fn llhttp_settings_init(settings: *mut llhttp_settings_t);

    // pub fn llhttp_finish(parser: *mut llhttp_t) -> llhttp_errno_t;
    // pub fn llhttp_pause(parser: *mut llhttp_t);
    
    // pub fn llhttp_resume(parser: *mut llhttp_t);
    pub fn llhttp_init(parser: *mut llhttp_t, type_: llhttp_type_t, settings: *const llhttp_settings_t);
    pub fn llhttp_settings_new() -> *mut llhttp_settings_t;
    pub fn llhttp_settings_free(settings: *const llhttp_settings_t);
    pub fn llhttp_errno_name(err: llhttp_errno_t) -> *const c_char;
}
