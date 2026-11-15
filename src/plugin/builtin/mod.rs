pub mod age_filter;
pub mod cleaner;
pub mod keyword;
pub mod limit;
pub mod link_cleaner;
pub mod nop;
pub mod regex_filter;

pub use age_filter::AgeFilterPlugin;
pub use cleaner::FeedCleanerPlugin;
pub use keyword::KeywordFilterPlugin;
pub use limit::LimitPlugin;
pub use link_cleaner::LinkCleanerPlugin;
pub use nop::NopHandlerPlugin;
pub use regex_filter::RegexFilterPlugin;
