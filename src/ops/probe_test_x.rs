#[test]
fn probe_http() {
    println!("expr(http_server) = {:?}", crate::ops::fts_match_expr("http_server"));
    println!("expr(httpServerUtil) = {:?}", crate::ops::fts_match_expr("httpServerUtil"));
}
