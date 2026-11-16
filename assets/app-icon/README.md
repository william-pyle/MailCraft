# MailCraft app icon

A solid white envelope on the same solid blue rounded tile used by ContactCraft's icon. The
MailCraft envelope is drawn as a simple vector mark, with no gradients, shadows or extra objects.

`source/mailcraft.svg` is the source. Run `cargo xtask icon` to regenerate Windows `.ico`, macOS
`.icns`, and Linux PNG and SVG files. The generator keeps the platform sizes and macOS margin.
Older alternate calendar-envelope studies in `alternates/` are retained as design history; they
are not used for the current icon.
