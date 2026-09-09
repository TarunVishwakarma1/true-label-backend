use crate::error::AppError;

pub fn validate_barcode(barcode: &str) -> Result<(), AppError> {
    if barcode.len() < 8 || barcode.len() > 14 {
        return Err(AppError::InvalidBarcode);
    }
    if !barcode.chars().all(|c| c.is_numeric()) {
        return Err(AppError::InvalidBarcode);
    }
    Ok(())
}

pub fn validate_country(country: &str) -> Result<(), AppError> {
    if country.len() != 2 || !country.chars().all(|c| c.is_alphabetic()) {
        return Err(AppError::InvalidCountry);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_barcode_length_and_digits() {
        assert!(validate_barcode("12345678").is_ok());
        assert!(validate_barcode("1234567").is_err());
        assert!(validate_barcode("123456789012345").is_err());
        assert!(validate_barcode("1234567a").is_err());
    }

    #[test]
    fn validates_country_code() {
        assert!(validate_country("IN").is_ok());
        assert!(validate_country("USA").is_err());
        assert!(validate_country("1N").is_err());
    }
}
