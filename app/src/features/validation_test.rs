//! Simple compilation test for validation module

#[cfg(test)]
mod validation_compile_test {
    use super::super::validation::*;
    use super::super::types::*;
    use crate::config::Environment;
    
    #[test]
    fn test_validation_compiles() {
        let validator = FeatureFlagValidator::new();
        let flag = FeatureFlag::new("test_flag".to_string(), true);
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_ok());
    }
    
    #[test] 
    fn test_validation_context() {
        let context = ValidationContext {
            environment: Environment::Development,
            schema_version: "1.0".to_string(),
            strict_mode: false,
            deprecated_warnings: true,
        };
        
        let validator = FeatureFlagValidator::with_context(context);
        assert!(true); // Just test that it compiles
    }
}