pub mod auth;
pub mod models;
pub mod socket;
pub mod build;
pub mod helpers;
pub mod error_success;
pub mod pending_update;
pub mod commands;
pub mod ssl;
pub mod handle_config;
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
