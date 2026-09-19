all: fonts fonts-code

dist:
	mkdir -p dist

fonts: dist icons
	npx fantasticon icons/ -o dist/ -t ttf -g json
	cp dist/icons.ttf fonts/icons.ttf

# Generate src/icons.rs into a temp file first so a failing tool never
# clobbers the committed file.
fonts-code: fonts
	@jq -r 'to_entries[] | "\(.key | ascii_upcase | gsub("-"; "_")) \(.value)"' dist/icons.json | while read name code; do \
		printf 'pub const %s: &str = "\\u{%x}";\n' "$$name" "$$code"; \
	done > src/icons.rs.tmp && mv src/icons.rs.tmp src/icons.rs
