pub const PRELUDE_MODULE: &str = "som";
pub const ENTRY_POINT: &str = "main";
pub const SELF_VALUE: &str = "self";
pub const SELF_TYPE: &str = "Self";
pub const FAIL_EVENT: &str = "fail";

pub const MEM_MODULE: &str = "som::mem";
pub const FORGET: &str = "forget";

pub const LEN: &str = "len";
pub const BYTES: &str = "bytes";
pub const RANGE_START: &str = "start";
pub const RANGE_END: &str = "end";

pub fn is_std_module(name: &str) -> bool {
    name == PRELUDE_MODULE
        || name
            .strip_prefix(PRELUDE_MODULE)
            .is_some_and(|rest| rest.starts_with("::"))
}
