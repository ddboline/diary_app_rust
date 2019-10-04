pub mod app;
pub mod errors;
pub mod logged_user;
pub mod requests;
pub mod routes;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
