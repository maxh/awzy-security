//! Relay connections over a list a resolver already holds. Every list in either portal's API is
//! a connection (ADR 0012), so a page can start short and load more; this pages the lists a store
//! returns whole and bounded. A list that can be large pages in its store instead (decisions).

use async_graphql::{
    connection::{Connection, Edge},
    Error, OutputType,
};

use crate::ErrorCode;

/// Items per page when the query doesn't say.
pub const DEFAULT_PAGE: usize = 20;
/// The most items one page returns.
pub const MAX_PAGE: usize = 200;

/// Pages `items`, already in order, by each one's stable key: a page's cursors are its items'
/// keys, and `after` continues from the item with that key.
pub fn page<T: OutputType>(
    items: Vec<T>,
    key: impl Fn(&T) -> String,
    after: Option<String>,
    first: Option<i32>,
) -> Result<Connection<String, T>, Error> {
    let first = match first {
        None => DEFAULT_PAGE,
        Some(n) => usize::try_from(n).map_err(|_| ErrorCode::InvalidInput.error("first can't be negative"))?,
    }
    .min(MAX_PAGE);
    let start = match after {
        None => 0,
        Some(cursor) => {
            items
                .iter()
                .position(|item| key(item) == cursor)
                .ok_or_else(|| ErrorCode::InvalidInput.error("that cursor isn't in this list any more"))?
                + 1
        }
    };
    let mut connection = Connection::new(start > 0, items.len() > start + first);
    connection.edges.extend(items.into_iter().skip(start).take(first).map(|item| {
        let cursor = key(&item);
        Edge::new(cursor, item)
    }));
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(c: &Connection<String, String>) -> Vec<&str> {
        c.edges.iter().map(|e| e.node.as_str()).collect()
    }

    fn letters() -> Vec<String> {
        ["a", "b", "c", "d", "e"].map(String::from).to_vec()
    }

    #[test]
    fn a_page_continues_after_its_cursor_and_says_whether_more_follow() {
        let first = page(letters(), Clone::clone, None, Some(2)).unwrap();
        assert_eq!(keys(&first), ["a", "b"]);
        assert!(first.has_next_page && !first.has_previous_page);
        let next = page(letters(), Clone::clone, Some("b".into()), Some(2)).unwrap();
        assert_eq!(keys(&next), ["c", "d"]);
        let last = page(letters(), Clone::clone, Some("d".into()), Some(2)).unwrap();
        assert_eq!(keys(&last), ["e"]);
        assert!(!last.has_next_page && last.has_previous_page);
    }

    #[test]
    fn a_cursor_that_left_the_list_is_refused_rather_than_restarting() {
        assert!(page(letters(), Clone::clone, Some("z".into()), Some(2)).is_err());
        assert!(page(letters(), Clone::clone, None, Some(-1)).is_err());
    }
}
