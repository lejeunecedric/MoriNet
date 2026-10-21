// Zoom bar for mdBook pages — ported from oscbrain (oscbrain.hollitech.fr).
// Scales the main content AND the sidebar in lockstep, so the layout stays
// aligned instead of overlapping. The level is persisted in localStorage.
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
    function build() {
        if (document.getElementById("km-zoom-bar")) { apply(); return; }
        var bar = document.createElement("div");
        bar.id = "km-zoom-bar";
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
        bar.appendChild(mk("&minus;", "Zoom out", function () { current = clamp(current - STEP); apply(); }));
        var val = document.createElement("span");
        val.id = "km-zoom-val";
        bar.appendChild(val);
        bar.appendChild(mk("&plus;", "Zoom in", function () { current = clamp(current + STEP); apply(); }));
        bar.appendChild(mk("&#x21BA;", "Reset zoom", function () { current = 1; apply(); }, "km-zoom-reset"));
        document.body.appendChild(bar);
        apply();
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", build);
    } else {
        build();
    }
})();
