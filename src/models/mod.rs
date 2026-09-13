pub mod ocr;
pub mod product;
pub mod response;
pub mod verification;

pub use ocr::{OcrResponse, OcrSubmission, SubmitLabelRequest};
pub use product::{
    AlternativesQuery, NeedsVerificationQuery, Product, ProductResponse, ProductSummary,
    SearchProductQuery, VerificationCandidate,
};
pub use response::{ApiResponse, HealthResponse, ReadinessResponse, ServiceStatus};
pub use verification::{Verification, VerifyProductRequest};
