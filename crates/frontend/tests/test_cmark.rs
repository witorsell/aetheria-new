#[test]
fn test_cmark() {
    let md = "![](https://cdn.imgchest.com/files/dbb29f5f2455.png)";
    let parser = pulldown_cmark::Parser::new_ext(md, pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    for event in parser {
        println!("{:?}", event);
    }
}
