/// merge consecutive chars to one char
/// # Example
/// ```
/// assert_eq!("/a/b", dce_util::string::merge_consecutive_char("//a///b", '/'))
/// ```
pub fn merge_consecutive_char(str: &str, target: char) -> String {
    let mut result = "".to_string();
    let mut last_char = '\0';
    for char in str.chars() {
        if char != target || last_char != target {
            result.push(char);
        }
        last_char = char;
    }
    result
}