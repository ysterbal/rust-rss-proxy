// filter_sponsors.rhai
// Drops items that look like ads, sponsored content, or marketing fluff.

fn filter_item(item) {
    let lower_title = item.title.to_lower();
    let lower_desc = item.description.to_lower();

    // Check for sponsored keywords
    if lower_title.contains("sponsored") || lower_desc.contains("sponsored by") {
        return "drop";
    }

    if lower_title.contains("partner post") || lower_title.contains("[promoted]") {
        return "drop";
    }

    // Check categories/tags
    for cat in item.categories {
        let lower_cat = cat.to_lower();
        if lower_cat == "sponsored" || lower_cat == "ads" || lower_cat == "promotion" {
            return "drop";
        }
    }

    return "keep";
}
