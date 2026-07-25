mod error;
mod request;
mod response;

pub use error::{ApiError, ErrorCode, FieldViolation, ProblemDetails};
pub use request::{SearchRequest, ValidatedSearchRequest};
pub use response::{EngineCatalogueResponse, EngineDescriptorResponse, SearchResponse, SearxCompatResponse};
