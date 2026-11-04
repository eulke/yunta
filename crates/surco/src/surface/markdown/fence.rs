//! Markdown an author wrote, read for the fences it leaves open: a block that
//! ends inside a fence it opened is closed where it ends, so it does not run on
//! into what follows it. Fences are read as CommonMark reads them, in list
//! items too.

/// The text as it is, followed, when it ends inside a fence it opened, by the
/// line that closes that fence, so that such a block does not run on into what
/// follows it. The caller trims the text first.
///
/// Fences are read as CommonMark reads them, in list items too: a line not
/// indented into an item ends it, and its fence, unless it continues a
/// paragraph lazily. The closing line takes the opening line's indentation.
/// Not read: fences on list marker lines, tabs as indentation, HTML blocks, and
/// block quote content beyond the paragraph a quoted line leaves open.
pub(super) fn closed(text: &str) -> String {
    match crate::code::fence_left_open(text) {
        Some(closer) => format!("{text}\n{closer}"),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::closed;

    /// Text that ends inside a backtick fence gets a line of three backticks.
    #[test]
    fn a_text_ending_inside_a_backtick_fence_gets_its_closing_line() {
        assert_eq!(
            closed("Intro.\n```rust\nlet a = 1;"),
            "Intro.\n```rust\nlet a = 1;\n```"
        );
    }

    /// Text whose fences all close comes back as it is.
    #[test]
    fn a_text_whose_fences_all_close_comes_back_unchanged() {
        let text = "```rust\nlet a = 1;\n```\n\nSee `a` here.\n\n~~~\nb\n~~~";
        assert_eq!(closed(text), text);
    }

    /// A fence of four backticks is closed by four, not by a line of three.
    #[test]
    fn a_fence_of_four_backticks_is_closed_only_by_four() {
        assert_eq!(
            closed("````\n```\nlet b = 2;"),
            "````\n```\nlet b = 2;\n````"
        );
        assert_eq!(closed("````\n```\n````"), "````\n```\n````");
    }

    /// A tilde fence is closed by tildes, and backticks inside it are code.
    #[test]
    fn a_tilde_fence_is_closed_by_tildes_and_not_by_backticks() {
        assert_eq!(closed("~~~\nb\n```\n~~~"), "~~~\nb\n```\n~~~");
        assert_eq!(closed("~~~\nb\n```"), "~~~\nb\n```\n~~~");
    }

    /// Backticks with another backtick after their run open no fence.
    #[test]
    fn backticks_with_another_backtick_after_their_run_open_no_fence() {
        assert_eq!(closed("```a``` is inline code."), "```a``` is inline code.");
    }

    /// A line indented by four spaces is code, not a fence.
    #[test]
    fn a_fence_indented_by_four_spaces_opens_no_fence() {
        assert_eq!(closed("    ```\nlet a = 1;"), "    ```\nlet a = 1;");
    }

    /// A fence in a list item is closed by a line at the item's indentation.
    #[test]
    fn a_fence_in_a_list_item_is_closed_at_its_indentation() {
        assert_eq!(
            closed("- a\n\n  ```bash\n  x"),
            "- a\n\n  ```bash\n  x\n  ```"
        );
        assert_eq!(
            closed("1. a\n\n   ```\n   x"),
            "1. a\n\n   ```\n   x\n   ```"
        );
    }

    /// A run followed by text is code, so it leaves its fence open.
    #[test]
    fn a_run_followed_by_text_leaves_its_fence_open() {
        assert_eq!(closed("```\ncode\n``` x"), "```\ncode\n``` x\n```");
    }

    /// A closing run indented by four spaces is code, not the fence's end.
    #[test]
    fn a_closing_run_indented_by_four_spaces_is_code() {
        assert_eq!(closed("```\ncode\n    ```"), "```\ncode\n    ```\n```");
    }

    /// A closing run may be followed by a tab, as CommonMark allows.
    #[test]
    fn a_closing_run_may_be_followed_by_a_tab() {
        assert_eq!(closed("```\nx\n```\t\nafter"), "```\nx\n```\t\nafter");
    }

    /// A fence in a list item ends with the item, so text after the item is not in it.
    #[test]
    fn a_fence_in_a_list_item_ends_with_the_item() {
        let text = "- a\n\n  ```\n  x\n\nDone";
        assert_eq!(closed(text), text);
        let nested = "- a\n  - b\n\n    ```\n    x\n  y";
        assert_eq!(closed(nested), nested);
    }

    /// A lazy paragraph line does not end the list item that holds its fence.
    #[test]
    fn a_lazy_paragraph_line_keeps_the_list_item_open() {
        assert_eq!(closed("- a\nb\n  ```\n  x"), "- a\nb\n  ```\n  x\n  ```");
    }

    /// A break ends the items it is not indented into, and a quote's paragraph lets an item start.
    #[test]
    fn a_break_not_indented_into_a_list_item_ends_it() {
        assert_eq!(closed("- a\n# h\n  ```\nx"), "- a\n# h\n  ```\nx\n  ```");
        assert_eq!(closed("- a\n---\n  ```\nx"), "- a\n---\n  ```\nx\n  ```");
        assert_eq!(closed("- a\n> q\n  ```\nx"), "- a\n> q\n  ```\nx\n  ```");
        assert_eq!(closed("> q\n2. d\n   ```"), "> q\n2. d\n   ```\n   ```");
    }

    /// A setext underline ends the paragraph of its item, so a line after it is not lazy.
    #[test]
    fn a_setext_underline_ends_the_paragraph_of_its_item() {
        assert_eq!(
            closed("- x\n  -\nlazy\n  ```\n  y\nDone"),
            "- x\n  -\nlazy\n  ```\n  y\nDone\n  ```"
        );
    }

    /// A numbered item past one is text, so the fence after it stays in the
    /// bullet item above and gets its closing line at the fence's own indent.
    /// An item with no text ends at the blank line after its marker, so an
    /// indented fence after that line is code. A fence four spaces past its
    /// item's text is code too.
    #[test]
    fn a_numbered_item_past_one_is_text_and_four_spaces_is_code() {
        assert_eq!(
            closed("- a\n  b\n  2. c\n\n     ```\n     x\n   y"),
            "- a\n  b\n  2. c\n\n     ```\n     x\n   y\n     ```"
        );
        let blank_start = "- \n\n     ```";
        assert_eq!(closed(blank_start), blank_start);
        let four = "- a\n\n      ```\n      x";
        assert_eq!(closed(four), four);
    }

    /// A carriage return that ends a line is not part of the line.
    #[test]
    fn a_carriage_return_ending_a_line_is_not_part_of_it() {
        assert_eq!(closed("```\r\n```\r\nx"), "```\r\n```\r\nx");
    }
}
