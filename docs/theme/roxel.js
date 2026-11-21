(() => {
    const narrow = window.matchMedia("(max-width: 1079px)");
    const toggle = document.getElementById("mdbook-sidebar-toggle-anchor");
    const sidebar = document.getElementById("mdbook-sidebar");
    let collapsedByWidth = false;
    let automatic = false;

    const read = () => {
        try {
            return localStorage.getItem("mdbook-sidebar");
        } catch {
            return null;
        }
    };

    const write = (value) => {
        try {
            if (value === null) localStorage.removeItem("mdbook-sidebar");
            else localStorage.setItem("mdbook-sidebar", value);
        } catch {}
    };

    // book.js saves every toggle as the reader's choice; automatic changes
    // restore what was stored so they don't outlive the window width.
    const setOpen = (open) => {
        if (toggle.checked === open) return;
        const stored = read();
        automatic = true;
        // book.js hides the sidebar with display: none after it slides shut and
        // only restores it from the toggle button's click handler.
        if (open) sidebar.style.display = "";
        toggle.checked = open;
        toggle.dispatchEvent(new Event("change"));
        automatic = false;
        write(stored);
    };

    toggle.addEventListener("change", () => {
        if (!automatic) collapsedByWidth = false;
    });

    document.getElementById("mdbook-theme-navy")?.setAttribute("aria-label", "Dark");

    narrow.addEventListener("change", ({ matches }) => {
        if (matches && toggle.checked) {
            setOpen(false);
            collapsedByWidth = true;
        } else if (!matches && collapsedByWidth) {
            collapsedByWidth = false;
            setOpen(true);
        }
    });
})();
