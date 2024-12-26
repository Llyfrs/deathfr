// Nice way to define name for collections and not have to worry about the names.
pub trait CollectionName {
    fn collection_name() -> &'static str;
}
