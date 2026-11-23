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

(() => {
    const isImage = /\.(png|jpe?g|webp|gif)$/i;
    const links = [...document.querySelectorAll("main a")].filter(
        (a) => a.children.length === 1 && a.firstElementChild.tagName === "IMG" && isImage.test(a.pathname),
    );
    if (links.length === 0) return;

    const icon = (...paths) => {
        const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        svg.setAttribute("viewBox", "0 0 24 24");
        svg.setAttribute("aria-hidden", "true");
        for (const d of paths) {
            const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
            path.setAttribute("d", d);
            svg.append(path);
        }
        return svg;
    };

    const button = (className, label, ...paths) => {
        const el = document.createElement("button");
        el.type = "button";
        el.className = className;
        el.setAttribute("aria-label", label);
        el.append(icon(...paths));
        return el;
    };

    const dialog = document.createElement("dialog");
    dialog.className = "roxel-lightbox";
    const figure = document.createElement("figure");
    const image = document.createElement("img");
    const caption = document.createElement("figcaption");
    const close = button("roxel-lightbox-close", "Close", "M18 6 6 18", "m6 6 12 12");
    const prev = button("roxel-lightbox-prev", "Previous image", "m15 18-6-6 6-6");
    const next = button("roxel-lightbox-next", "Next image", "m9 18 6-6-6-6");
    figure.append(image, caption);
    dialog.append(figure, close, prev, next);
    document.body.append(dialog);

    let group = [];
    let index = 0;

    const show = (i) => {
        index = (i + group.length) % group.length;
        const link = group[index];
        const thumb = link.firstElementChild;
        const dark = document.documentElement.matches(".navy, .coal, .ayu") && link.dataset.darkHref;
        image.src = dark ? new URL(link.dataset.darkHref, document.baseURI).href : link.href;
        image.alt = thumb.alt;
        caption.textContent = thumb.alt;
    };

    const step = (delta) => {
        if (group.length > 1) show(index + delta);
    };

    for (const link of links) {
        link.addEventListener("click", (event) => {
            if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
            event.preventDefault();
            const list = link.closest("ul");
            group = list ? links.filter((a) => list.contains(a)) : [link];
            dialog.classList.toggle("roxel-lightbox-single", group.length === 1);
            show(group.indexOf(link));
            dialog.showModal();
        });
    }

    close.addEventListener("click", () => dialog.close());
    prev.addEventListener("click", () => step(-1));
    next.addEventListener("click", () => step(1));
    dialog.addEventListener("click", (event) => {
        if (event.target === dialog || event.target === figure) dialog.close();
    });
    // book.js turns arrow keys into chapter navigation from a document listener.
    dialog.addEventListener("keydown", (event) => {
        event.stopPropagation();
        if (event.key === "ArrowLeft") step(-1);
        if (event.key === "ArrowRight") step(1);
    });
    dialog.addEventListener("close", () => {
        image.removeAttribute("src");
    });
})();
