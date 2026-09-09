use crate::{
    error::Result,
    models::{OcrResponse, SubmitLabelRequest},
    state::AppState,
};
use axum::{extract::State, Json};

pub async fn submit_label(
    State(state): State<AppState>,
    Json(body): Json<SubmitLabelRequest>,
) -> Result<Json<OcrResponse>> {
    let response = state.ocr_service.submit_label(&body).await?;
    Ok(Json(response))
}
