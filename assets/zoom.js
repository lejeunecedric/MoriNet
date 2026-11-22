// Vault chrome for mdBook pages (REQ-NAV-003 + zoom).
// The zoom widget is ported from oscbrain (oscbrain.hollitech.fr): it scales
// the main content AND the sidebar in lockstep, so the layout stays aligned
// instead of overlapping. The level is persisted in localStorage.
// Both controls live in ONE fixed toolbar at the top right, below mdBook's
// print button (see zoom.css: top 44px clears the print row).
(function () {
    var MIN = 0.5, MAX = 3.0, STEP = 0.1;
    var KEY = "morinet-zoom";
    var baseW = null;
    function clamp(v) { return Math.min(MAX, Math.max(MIN, Math.round(v * 10) / 10)); }
    var current = clamp(parseFloat(localStorage.getItem(KEY)) || 1);
    function apply() {
        try { localStorage.setItem(KEY, String(current)); } catch (e) {}
        var mains = document.querySelectorAll("main");
        for (var i = 0; i < mains.length; i++) {
            mains[i].style.zoom = current;
        }
        var sb = document.getElementById("mdbook-sidebar");
        if (!sb) sb = document.querySelector(".sidebar");
        if (sb) {
            if (baseW === null) {
                baseW = sb.getBoundingClientRect().width || 300;
            }
            if (current === 1) {
                sb.style.fontSize = "";
                document.documentElement.style.removeProperty("--sidebar-width");
            } else {
                sb.style.fontSize = (0.875 * current) + "em";
                document.documentElement.style.setProperty("--sidebar-width", Math.round(baseW * current) + "px");
            }
        }
        var val = document.getElementById("km-zoom-val");
        if (val) val.textContent = Math.round(current * 100) + "%";
    }
    function mk(html, label, fn, id) {
        var b = document.createElement("button");
        b.type = "button";
        b.innerHTML = html;
        b.title = label;
        b.setAttribute("aria-label", label);
        if (id) b.id = id;
        b.addEventListener("click", fn);
        return b;
    }
    function build() {
        if (document.getElementById("km-zoom-bar")) { apply(); return; }
        var bar = document.createElement("div");
        bar.id = "km-zoom-bar";

        bar.appendChild(mk("&minus;", "Zoom out", function () { current = clamp(current - STEP); apply(); }));
        var val = document.createElement("span");
        val.id = "km-zoom-val";
        bar.appendChild(val);
        bar.appendChild(mk("&plus;", "Zoom in", function () { current = clamp(current + STEP); apply(); }));
        bar.appendChild(mk("&#x21BA;", "Reset zoom", function () { current = 1; apply(); }, "km-zoom-reset"));

        // "Close vault" — back to MoriNet's "My vaults" (the app root). This
        // is the bug fix: the old link pointed at the vault's own index.
        var close = document.createElement("a");
        close.id = "morinet-close-vault";
        close.href = "/";
        close.title = "Close this vault (back to My vaults)";
        close.setAttribute("aria-label", "Close this vault");
        close.innerHTML = "&#x2715;&nbsp; Close vault";
        bar.appendChild(close);

        document.body.appendChild(bar);
        apply();
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", build);
    } else {
        build();
    }
})();

// ---------------------------------------------------------------------------
// "Looks" color-scheme picker (issue #17, UI-02 / D-05).
// Injects a MoriNet-owned "Looks" section into mdBook's own theme popup
// (#mdbook-theme-list) offering the five mdBook palettes. Picking one writes
// mdBook's OWN localStorage key ('mdbook-theme') and reloads the page, so
// mdBook's own get_theme() applies it — visitor localStorage wins over the
// vault's serve-time theme default (Phase 12 D-08/D-10). Zero new CSS: the
// items reuse the popup's own li / button.theme markup vocabulary. Degrades
// silently on pages whose mdBook version lacks the popup markup.
(function () {
    var PALETTES = [
        ["light", "Light", "morinet-looks-light"],
        ["navy", "Navy", "morinet-looks-navy"],
        ["rust", "Rust", "morinet-looks-rust"],
        ["coal", "Coal", "morinet-looks-coal"],
        ["ayu", "Ayu", "morinet-looks-ayu"]
    ];
    function pick(palette) {
        return function (ev) {
            // mdBook 0.5.4's popup-level click listener also fires for these
            // buttons (className "theme" inside #mdbook-theme-list) and would
            // call set_theme('morinet-looks-*'), clobbering mdbook-theme with
            // an invalid value. Stop the event before writing our own value.
            if (ev) { ev.preventDefault(); ev.stopPropagation(); }
            try { localStorage.setItem('mdbook-theme', palette); } catch (e) {}
            // Reload is the zero-coupling way to make book.js apply the
            // theme — we never call into book.js internals (21-PATTERNS §5).
            window.location.reload();
        };
    }
    function findAboutAnchor() {
        var about = document.getElementById("about-btn");
        if (about) return about;
        var bar = document.getElementById("mdbook-menu-bar");
        if (!bar) return null;
        var nodes = bar.querySelectorAll("button, a");
        for (var i = 0; i < nodes.length; i++) {
            var t = (nodes[i].textContent || "").trim();
            if (t === "About") return nodes[i];
        }
        return null;
    }
    function build() {
        if (document.getElementById("morinet-looks-light")) return;
        var popup = document.getElementById("mdbook-theme-list");
        if (!popup) return; // older/newer mdBook, or non-book page: stay silent
        var saved = null;
        try { saved = localStorage.getItem("mdbook-theme"); } catch (e) {}
        var frag = document.createDocumentFragment();
        var head = document.createElement("li");
        head.textContent = "Looks";
        frag.appendChild(head);
        for (var i = 0; i < PALETTES.length; i++) {
            var li = document.createElement("li");
            li.setAttribute("role", "none");
            var b = document.createElement("button");
            b.type = "button";
            b.setAttribute("role", "menuitem");
            b.className = "theme";
            b.id = PALETTES[i][2];
            b.textContent = PALETTES[i][1];
            if (saved && saved === PALETTES[i][0]) {
                b.setAttribute("aria-current", "true");
            }
            b.addEventListener("click", pick(PALETTES[i][0]));
            li.appendChild(b);
            frag.appendChild(li);
        }
        // D-05 "before About": anchor on an About control when one exists
        // (this mdBook version renders none — the popup IS the settings menu),
        // else insert at the top of the theme popup (before the Auto item).
        var about = findAboutAnchor();
        if (about && about.parentNode) {
            about.parentNode.insertBefore(frag, about);
        } else {
            popup.insertBefore(frag, popup.firstChild);
        }
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", build);
    } else {
        build();
    }
})();
