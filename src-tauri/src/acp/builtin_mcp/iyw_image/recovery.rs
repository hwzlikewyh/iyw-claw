use super::{ImageExecution, ImageRequest, ImageResult, IywGatewayService};

mod output;
mod prepare;
mod retry;

pub(super) use prepare::{prepare_candidates, validate_models};

pub(super) async fn execute(
    service: &IywGatewayService,
    initial: ImageExecution<'_>,
    candidates: &[ImageRequest],
) -> Result<ImageResult, rmcp::ErrorData> {
    let mut attempts = Vec::new();
    for (index, execution) in executions(initial, candidates).enumerate() {
        let (result, mut decision) = retry::execute(
            service,
            execution,
            retry::Candidate {
                index,
                attempts: &mut attempts,
            },
        )
        .await;
        if decision == "fallback" && index == candidates.len() {
            decision = "chain_exhausted";
        }
        if decision != "fallback" {
            if candidates.is_empty() && attempts.len() == 1 {
                return result;
            }
            if let Some(last) = attempts.last_mut() {
                last["decision"] = serde_json::json!(decision);
            }
            return output::finish(result, attempts, decision);
        }
    }
    unreachable!("the last candidate always stops")
}

fn executions<'a>(
    initial: ImageExecution<'a>,
    candidates: &'a [ImageRequest],
) -> impl Iterator<Item = ImageExecution<'a>> {
    let images = initial.images;
    std::iter::once(initial).chain(candidates.iter().map(move |request| ImageExecution {
        request,
        kind: request.kind.as_deref().expect("candidate type validated"),
        images,
    }))
}
