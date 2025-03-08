fix:
    cargo fmt --all
    cargo clippy --fix --allow-dirty

check:
    cargo clippy 
    cargo fmt --check

download-syntax-themes:
    curl -sLO --output-dir src/vendor/ "https://raw.githubusercontent.com/Colorsublime/Colorsublime-Themes/refs/heads/master/themes/GitHub_Dark.tmTheme"
    curl -sLO --output-dir src/vendor/ "https://raw.githubusercontent.com/Colorsublime/Colorsublime-Themes/refs/heads/master/themes/GitHub_Light.tmTheme"
