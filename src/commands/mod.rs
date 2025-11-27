use anyhow::Result;

use crate::state::Ctx;

pub mod member;
pub mod settings;
pub mod stats;
pub mod userinfo;

pub const MAX_EMBED_DESCRIPTION_CHARS: usize = 4096;

/// Split lines into description chunks, each <= max_chars (counted in Unicode scalar values).
pub fn chunk_lines(lines: &[String], max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for line in lines {
        let line_len = line.chars().count();
        // +1 for the newline if current is not empty
        let extra = if current.is_empty() {
            line_len
        } else {
            line_len + 1
        };

        if current_len + extra > max_chars {
            if !current.is_empty() {
                chunks.push(current);
            }
            current = line.clone();
            current_len = line_len;
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
            current_len += extra;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Generic helper:
/// - `lines` → will be joined into descriptions (split into chunks).
/// - `build_first` → called for the first chunk; lets you add thumbnail/fields/etc.
/// - `build_cont`  → called for each continuation chunk with `(index, chunk)`.
pub async fn send_chunked_embeds<BF, BC>(
    ctx: Ctx<'_>,
    lines: Vec<String>,
    build_first: BF,
    build_cont: BC,
) -> Result<()>
where
    BF: FnOnce(String) -> serenity::all::CreateEmbed,
    BC: Fn(usize, String) -> serenity::all::CreateEmbed,
{
    use poise::CreateReply;

    let chunks = chunk_lines(&lines, MAX_EMBED_DESCRIPTION_CHARS);
    if chunks.is_empty() {
        // Caller usually checks, but being defensive.
        return Ok(());
    }

    // First embed
    let first_desc = chunks[0].clone();
    let first_embed = build_first(first_desc);
    ctx.send(CreateReply::default().embed(first_embed)).await?;

    // Continuations
    if chunks.len() > 1 {
        for (idx, chunk) in chunks.into_iter().enumerate().skip(1) {
            let embed = build_cont(idx, chunk);
            ctx.send(CreateReply::default().embed(embed)).await?;
        }
    }

    Ok(())
}
