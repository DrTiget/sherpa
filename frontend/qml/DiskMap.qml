import QtQuick
import Sherpa

// Карта размеров каталога: treemap или sunburst. node: {path, name, size, children: [node]}; клик по блоку — signal drill(node).
Item {
    id: map
    property var node: null
    property string mode: "treemap"          // treemap | sunburst
    property var hovered: null
    signal drill(var node)
    property var hits: []                    // [{x,y,w,h,node}] для treemap; для sunburst — {r0,r1,a0,a1,node}

    onNodeChanged: cv.requestPaint()
    onModeChanged: cv.requestPaint()
    onWidthChanged: cv.requestPaint()
    onHeightChanged: cv.requestPaint()
    Connections { target: Theme; function onModeChanged() { cv.requestPaint() } }

    function color(i, depth, shade) {
        const h = ((i * 0.137) % 1 + 0.55) % 1
        return Qt.hsla(h, Theme.light ? 0.5 : 0.55, (Theme.light ? (0.62 - depth * 0.06) : (0.42 - depth * 0.03)) + (shade || 0) * 0.07, 1)
    }
    function squarify(items, x, y, w, h, out, depth, idx0) {
        // items: [{node,size}] по убыванию; рекурсия в detail не нужна — рисуем один уровень + вложенные рамкой
        const total = items.reduce((s, it) => s + it.size, 0)
        if (total <= 0 || w < 2 || h < 2) return
        let rest = items.slice(), rx = x, ry = y, rw = w, rh = h, remaining = total
        while (rest.length) {
            const shortSide = Math.min(rw, rh)
            let row = [], rowSum = 0, best = Infinity
            for (let i = 0; i < rest.length; ++i) {
                const cand = row.concat([rest[i]]), s = rowSum + rest[i].size
                const side = shortSide, area = s / remaining * rw * rh
                const thick = area / side
                let worst = 0
                for (const c of cand) { const len = c.size / s * side; const r = Math.max(len / thick, thick / len); if (r > worst) worst = r }
                if (worst <= best || row.length === 0) { row = cand; rowSum = s; best = worst } else break
            }
            rest = rest.slice(row.length)
            const area = rowSum / remaining * rw * rh
            const horizontal = rw >= rh          // ряд вдоль короткой стороны
            const thick = area / shortSide
            let off = 0
            for (const it of row) {
                const len = it.size / rowSum * shortSide
                const r = horizontal ? { x: rx, y: ry + off, w: thick, h: len } : { x: rx + off, y: ry, w: len, h: thick }
                out.push({ x: r.x, y: r.y, w: r.w, h: r.h, node: it.node, idx: it.idx, shade: it.shade || 0 })
                off += len
            }
            if (horizontal) { rx += thick; rw -= thick } else { ry += thick; rh -= thick }
            remaining -= rowSum
        }
    }

    Canvas {
        id: cv
        anchors.fill: parent
        onPaint: {
            const c = getContext("2d")
            c.reset()
            map.hits = []
            const n = map.node
            if (!n || !n.children || !n.children.length || width < 40 || height < 40) return
            const kids = n.children.slice().sort((a, b) => b.size - a.size).filter(k => k.size > 0)
            const font = "11px " + Theme.fontUi
            c.font = font
            if (map.mode === "treemap") {
                const out = []
                const items = kids.map((k, i) => ({ node: k, size: k.size, idx: i }))
                map.squarify(items, 0, 0, width, height, out, 0, 0)
                for (const r of out) {
                    c.fillStyle = map.color(r.idx, 0)
                    c.fillRect(r.x + 1, r.y + 1, Math.max(0, r.w - 2), Math.max(0, r.h - 2))
                    // вложенные
                    const sub = (r.node.children || []).slice().sort((a, b) => b.size - a.size).filter(k => k.size > 0)
                    if (sub.length && r.w > 60 && r.h > 50) {
                        const inner = [], hdr = 16
                        map.squarify(sub.map((k, i) => ({ node: k, size: k.size, idx: r.idx, shade: i % 2 })), r.x + 4, r.y + hdr, r.w - 8, r.h - hdr - 4, inner, 1, r.idx)
                        for (const s of inner) {
                            c.fillStyle = map.color(s.idx, 1, s.shade)
                            c.fillRect(s.x + 1, s.y + 1, Math.max(0, s.w - 2), Math.max(0, s.h - 2))
                            if (s.w > 54 && s.h > 16) { c.fillStyle = Theme.alpha("#ffffff", 0.85); c.fillText(s.node.name, s.x + 5, s.y + 12, s.w - 8) }
                        }
                    }
                    if (r.w > 50 && r.h > 18) {
                        c.fillStyle = "#ffffff"; c.font = "bold " + font
                        c.fillText(r.w > 110 ? r.node.name + "  " + Fmt.bytes(r.node.size) : r.node.name, r.x + 6, r.y + 13, r.w - 10)
                        c.font = font
                    }
                    map.hits.push(r)
                }
            } else {
                const cx = width / 2, cy = height / 2, R = Math.min(cx, cy) - 4
                const rings = 2, r0 = R * 0.22, rw = (R - r0) / rings
                const total = kids.reduce((s, k) => s + k.size, 0)
                const hits = []
                let a = -Math.PI / 2
                kids.forEach((k, i) => {
                    const span = k.size / total * Math.PI * 2
                    const draw = (rin, rout, a0, a1, col) => {
                        c.beginPath(); c.arc(cx, cy, rout, a0, a1); c.arc(cx, cy, rin, a1, a0, true); c.closePath()
                        c.fillStyle = col; c.fill(); c.strokeStyle = Theme.light ? "#ffffff" : "#10141f"; c.lineWidth = 1; c.stroke()
                    }
                    if (span > 0.002) {
                        draw(r0, r0 + rw, a, a + span, map.color(i, 0))
                        hits.push({ r0: r0, r1: r0 + rw, a0: a, a1: a + span, node: k })
                        const sub = (k.children || []).slice().sort((x, y) => y.size - x.size).filter(s => s.size > 0)
                        let b = a
                        const subTotal = Math.max(k.size, sub.reduce((s, q) => s + q.size, 0))
                        let si = 0
                        for (const s of sub) {
                            const sp = s.size / subTotal * span
                            if (sp > 0.002) { draw(r0 + rw, R, b, b + sp, map.color(i, 1, si % 2)); hits.push({ r0: r0 + rw, r1: R, a0: b, a1: b + sp, node: s }) }
                            b += sp; si++
                        }
                        if (span > 0.35) {
                            const m = a + span / 2, rr = r0 + rw / 2
                            c.save(); c.translate(cx + Math.cos(m) * rr, cy + Math.sin(m) * rr); c.fillStyle = "#ffffff"; c.font = font
                            const t = k.name; c.fillText(t, -Math.min(rw * 0.9, c.measureText(t).width) / 2, 4, rw * 0.9); c.restore()
                        }
                    }
                    a += span
                })
                // центр
                c.beginPath(); c.arc(cx, cy, r0 - 3, 0, Math.PI * 2); c.fillStyle = Theme.alpha(Theme.text, 0.06); c.fill()
                c.fillStyle = Theme.text; c.font = "bold 12px " + Theme.fontUi
                const t1 = n.name || n.path, t2 = Fmt.bytes(n.size)
                c.fillText(t1, cx - Math.min(r0 * 1.7, c.measureText(t1).width) / 2, cy - 2, r0 * 1.7)
                c.fillStyle = Theme.textSub; c.font = font
                c.fillText(t2, cx - c.measureText(t2).width / 2, cy + 13)
                map.hits = hits
            }
        }
    }
    function hitAt(px, py) {
        if (mode === "treemap") {
            let best = null
            for (const r of hits) if (px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h) best = r
            // вложенный блок — точнее: ищем среди детей внутри
            if (best) {
                const kids = (best.node.children || []).slice().sort((a, b) => b.size - a.size).filter(k => k.size > 0)
                if (kids.length && best.w > 60 && best.h > 50) {
                    const inner = []
                    squarify(kids.map(k => ({ node: k, size: k.size, idx: best.idx })), best.x + 4, best.y + 16, best.w - 8, best.h - 20, inner, 1, best.idx)
                    for (const s of inner) if (px >= s.x && px <= s.x + s.w && py >= s.y && py <= s.y + s.h) return s.node
                }
                return best.node
            }
            return null
        }
        const cx = width / 2, cy = height / 2
        const dx = px - cx, dy = py - cy, r = Math.sqrt(dx * dx + dy * dy)
        let a = Math.atan2(dy, dx); if (a < -Math.PI / 2) a += Math.PI * 2
        for (const h of hits) if (r >= h.r0 && r <= h.r1 && a >= h.a0 && a <= h.a1) return h.node
        return null
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onPositionChanged: (m) => map.hovered = map.hitAt(m.x, m.y)
        onExited: map.hovered = null
        onClicked: (m) => { const n = map.hitAt(m.x, m.y); if (n) map.drill(n) }
        cursorShape: map.hovered ? Qt.PointingHandCursor : Qt.ArrowCursor
    }
}
