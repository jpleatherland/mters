use ropey::{
    str_utils::{byte_to_char_idx, char_to_byte_idx},
    Rope,
};
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

/// ------ Internal byte/char helpers (no allocation) -------------------------

#[inline]
fn abs_char_to_abs_byte(text: &Rope, ci: usize) -> usize {
    let (chunk, byte_start, char_start, _) = text.chunk_at_char(ci);
    let local_char = ci - char_start;
    byte_start + char_to_byte_idx(chunk, local_char)
}

#[inline]
fn abs_byte_to_abs_char(text: &Rope, bi: usize) -> usize {
    let (chunk, byte_start, char_start, _) = text.chunk_at_byte(bi);
    let local_byte = bi - byte_start;
    char_start + byte_to_char_idx(chunk, local_byte)
}

#[inline]
fn line_bounds_bytes(text: &Rope, row: usize) -> (usize, usize) {
    let start_ci = text.line_to_char(row);
    let end_ci = text.line_to_char(row + 1);
    (
        abs_char_to_abs_byte(text, start_ci),
        abs_char_to_abs_byte(text, end_ci),
    )
}

/// Step to next/prev grapheme *byte* boundary using GraphemeCursor and Ropey chunks.
fn step_grapheme_bound(text: &Rope, from_byte: usize, forward: bool) -> usize {
    let total_bytes = text.len_bytes();
    let mut cursor = GraphemeCursor::new(from_byte, total_bytes, /* extended */ true);

    // Start at the chunk containing `from_byte`.
    let (mut chunk, mut chunk_start, _, _) = text.chunk_at_byte(from_byte);

    loop {
        let res = if forward {
            cursor.next_boundary(chunk, chunk_start)
        } else {
            cursor.prev_boundary(chunk, chunk_start)
        };

        match res {
            Ok(Some(bi)) => return bi,
            Ok(None) => return if forward { total_bytes } else { 0 },

            Err(GraphemeIncomplete::PreContext(req_end)) => {
                // Provide pre-context ending exactly at `req_end`.
                let (ctx_chunk, ctx_start, _, _) = text.chunk_at_byte(req_end);
                let prefix_len = req_end - ctx_start;
                cursor.provide_context(&ctx_chunk[..prefix_len], ctx_start);
            }
            Err(GraphemeIncomplete::NextChunk) => {
                let next_start = chunk_start + chunk.len();
                if next_start >= total_bytes {
                    return total_bytes;
                }
                let (next_chunk, next_chunk_start, _, _) = text.chunk_at_byte(next_start);
                chunk = next_chunk;
                chunk_start = next_chunk_start;
            }
            Err(GraphemeIncomplete::PrevChunk) => {
                if chunk_start == 0 {
                    return 0;
                }
                let prev_probe = chunk_start - 1;
                let (prev_chunk, prev_chunk_start, _, _) = text.chunk_at_byte(prev_probe);
                chunk = prev_chunk;
                chunk_start = prev_chunk_start;
            }
            Err(GraphemeIncomplete::InvalidOffset) => {
                // Re-sync to the chunk containing the cursor's current byte position.
                let pos = cursor.cur_cursor();
                let (c, cs, _, _) = text.chunk_at_byte(pos);
                chunk = c;
                chunk_start = cs;
                // and loop to retry
            }
        }
    }
}

/// ------ Public: allocation-free next/prev grapheme at absolute char index ----

/// Next grapheme boundary (absolute *char* index) from an absolute *char* index.
/// If already at end, returns `text.len_chars()`.
pub fn next_grapheme_abs_char(text: &Rope, abs_ci: usize) -> usize {
    let from_byte = abs_char_to_abs_byte(text, abs_ci);
    let next_byte = step_grapheme_bound(text, from_byte, true);
    abs_byte_to_abs_char(text, next_byte)
}

/// Previous grapheme boundary (absolute *char* index) before an absolute *char* index.
/// If at start, returns 0.
pub fn prev_grapheme_abs_char(text: &Rope, abs_ci: usize) -> usize {
    let from_byte = abs_char_to_abs_byte(text, abs_ci);
    let prev_byte = step_grapheme_bound(text, from_byte, false);
    abs_byte_to_abs_char(text, prev_byte)
}

/// ------ Public: line-relative helpers (allocation-free) ---------------------

/// Count grapheme clusters on a line without allocating.
pub fn line_gcount(text: &Rope, row: usize) -> usize {
    let (sb, eb) = line_bounds_bytes(text, row);
    if sb == eb {
        return 0;
    }

    let mut count = 0usize;
    let mut b = sb;
    loop {
        let nb = step_grapheme_bound(text, b, true);
        if nb > eb {
            break;
        }
        count += 1;
        if nb == eb {
            break;
        }
        b = nb;
    }
    count
}

/// Convert (row, gcol) -> absolute *char* index, clamping gcol to end-of-line.
pub fn line_gcol_to_abs_char(text: &Rope, row: usize, mut gcol: usize) -> usize {
    let (sb, eb) = line_bounds_bytes(text, row);
    let gc = line_gcount(text, row);
    if gcol > gc {
        gcol = gc;
    }

    let mut b = sb;
    for _ in 0..gcol {
        let nb = step_grapheme_bound(text, b, true);
        if nb >= eb {
            b = eb;
            break;
        }
        b = nb;
    }
    abs_byte_to_abs_char(text, b)
}

/// Convert absolute *char* index -> (row, gcol), where gcol is grapheme offset within the line.
/// If `abs_ci` is between boundaries, we snap to the *previous* boundary (like cursor behavior).
pub fn abs_char_to_line_gcol(text: &Rope, abs_ci: usize) -> (usize, usize) {
    let row = text.char_to_line(abs_ci);
    let target_b = abs_char_to_abs_byte(text, abs_ci);
    let (sb, eb) = line_bounds_bytes(text, row);

    // Empty line => column 0
    if sb == eb {
        return (row, 0);
    }

    // Clamp into [sb, eb]
    let target_b = target_b.clamp(sb, eb);

    // Count grapheme boundaries from line start up to target_b.
    let mut gcol = 0usize;
    let mut b = sb;
    loop {
        let nb = step_grapheme_bound(text, b, true);

        // NEW: if the cursor can't advance (e.g., end-of-buffer == start), don't increment.
        if nb <= b {
            break;
        }

        if nb > target_b {
            break;
        }

        gcol += 1;

        if nb == target_b {
            break;
        }
        b = nb;
    }

    (row, gcol)
}
