// tag_and_prefix.rhai
// Demonstrates transforming items: prefixes titles based on categories or keywords

fn transform_item(item) {
    let lower_title = item.title.to_lower();

    if lower_title.contains("rust") && !item.title.starts_with("[Rust]") {
        item.title = "[Rust] " + item.title;
    } else if lower_title.contains("security") && !item.title.starts_with("[Security]") {
        item.title = "[Security] " + item.title;
    }

    return item;
}
