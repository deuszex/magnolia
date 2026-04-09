use magnolia_common::errors::AppError;

/// Validate password strength
/// Requirements:
/// - At least 12 characters
/// - Contains uppercase letter
/// - Contains lowercase letter
/// - Contains digit
/// - Contains special character
pub fn validate_password_strength(password: &str) -> Result<(), AppError> {
    if password.len() < 12 {
        return Err(AppError::BadRequest(
            "Password must be at least 12 characters long".to_string(),
        ));
    }

    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_digit = password.chars().any(|c| c.is_numeric());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    if !has_lowercase {
        return Err(AppError::BadRequest(
            "Password must contain at least one lowercase letter".to_string(),
        ));
    }

    if !has_uppercase {
        return Err(AppError::BadRequest(
            "Password must contain at least one uppercase letter".to_string(),
        ));
    }

    if !has_digit {
        return Err(AppError::BadRequest(
            "Password must contain at least one digit".to_string(),
        ));
    }

    if !has_special {
        return Err(AppError::BadRequest(
            "Password must contain at least one special character".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_password() {
        assert!(validate_password_strength("MyP@ssw0rd123!").is_ok());
    }

    #[test]
    fn test_too_short() {
        assert!(validate_password_strength("Short1!").is_err());
    }

    #[test]
    fn test_no_uppercase() {
        assert!(validate_password_strength("mypassword123!").is_err());
    }

    #[test]
    fn test_no_lowercase() {
        assert!(validate_password_strength("MYPASSWORD123!").is_err());
    }

    #[test]
    fn test_no_digit() {
        assert!(validate_password_strength("MyPassword!!!").is_err());
    }

    #[test]
    fn test_no_special() {
        assert!(validate_password_strength("MyPassword123").is_err());
    }
}
