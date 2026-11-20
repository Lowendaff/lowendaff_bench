//! friDay Video Taiwan streaming probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for friDay Video.
pub const SPEC: Platform = Platform {
    id: "friday",
    name: "friDay Video",
    probe,
};

/// Probes friDay Video over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url(concat!(
                "https://video.friday.tw/api2/streaming/get",
                "?streamingId=132287&streamingType=2&contentType=4&contentId=1&clientId="
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses friDay Video's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    // 1003 or "\u{6d77}\u{5916}" ("overseas") / "\u{5730}\u{533a}" ("region") marks geo-block.
    if body.contains("\"code\":1003")
        || body.contains("\u{6d77}\u{5916}")
        || body.contains("\u{5730}\u{533a}")
    {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("\"code\":") {
        return Verdict::yes_in("TW");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
