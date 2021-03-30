use stack_string::StackString;

#[derive(Debug, Clone)]
pub struct DirectoryInfo {
    pub directory_id: StackString,
    pub directory_name: StackString,
    pub parentid: Option<StackString>,
}
