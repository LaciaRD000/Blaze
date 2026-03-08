CREATE TABLE IF NOT EXISTS user_themes (
    user_id             INTEGER PRIMARY KEY,  -- Discord user ID
    color_scheme        TEXT NOT NULL DEFAULT 'base16-ocean.dark',
    background_id       TEXT NOT NULL DEFAULT 'default',
    blur_radius         REAL NOT NULL DEFAULT 8.0,
    opacity             REAL NOT NULL DEFAULT 0.75,
    font_family         TEXT NOT NULL DEFAULT 'Fira Code',
    font_size           REAL NOT NULL DEFAULT 14.0,
    title_bar_style     TEXT NOT NULL DEFAULT 'macos',
    show_line_numbers   INTEGER NOT NULL DEFAULT 0,  -- 0=OFF, 1=ON
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
