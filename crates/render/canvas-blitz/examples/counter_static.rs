//! Increment 1: a hardcoded counter-shaped DOM rendered in a Blitz window.
//! No som code involved yet — this only proves the window/layout/text/paint
//! pipeline works before we build the Renderer adapter on top of it.

const HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
<style type="text/css">
    html, body { height: 100%; margin: 0; }
    body {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 20px;
        background: #1e1e2e;
        color: #cdd6f4;
        font-family: sans-serif;
    }
    .count { font-size: 48px; font-weight: 700; }
    button {
        font-size: 20px;
        padding: 10px 24px;
        border: none;
        border-radius: 10px;
        background: #89b4fa;
        color: #1e1e2e;
    }
</style>
</head>
<body>
    <div class="count">Count: 0</div>
    <button>+1</button>
</body>
</html>
"#;

fn main() {
    blitz::launch_static_html(HTML);
}
