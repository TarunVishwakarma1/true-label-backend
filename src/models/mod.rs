pub mod ocr;
pub mod product;
pub mod response;
pub mod verification;

pub use ocr::{OcrResponse, OcrSubmission, SubmitLabelRequest};
pub use product::{AlternativesQuery, Product, ProductResponse, ProductSummary, SearchProductQuery};
pub use response::{ApiResponse, HealthResponse, ReadinessResponse, ServiceStatus};
pub use verification::{Verification, VerifyProductRequest};
