all: fonts fonts-code

dist:
	mkdir -p dist

fonts: dist icons
	npx fantasticon icons/ -o dist/ -t ttf -g json  

fonts-code: fonts
	echo "" > src/icons.rs
	@jq -r 'to_entries[] | "\(.key | ascii_upcase | gsub("-"; "_")) \(.value)"' dist/icons.json | while read name code; do \
		printf 'pub const %s: &str = "\\u{%x}";\n' "$$name" "$$code"; \
	done > src/icons.rs
