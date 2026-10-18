import { hierarchy, treemap, treemapSquarify } from "d3-hierarchy";
import { motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { human } from "../format";
import type { Node } from "../types";

interface Props { node: Node; selected: string | null; onSelect(n: Node): void; onOpen(n: Node): void }

/** Tek seviyelik treemap: doğrudan çocuklar boyutlarıyla orantılı kutular. */
export function Treemap({ node, selected, onSelect, onOpen }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = box.current!;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const cells = useMemo(() => {
    if (!size.w || !size.h || !node.children.length) return [];
    const root = hierarchy<{ c?: Node }>({ c: undefined, ...{ children: node.children.map((c) => ({ c })) } } as never)
      .sum((d) => d.c?.size ?? 0);
    treemap<{ c?: Node }>().tile(treemapSquarify.ratio(1.2)).size([size.w, size.h]).paddingInner(3).round(true)(root);
    return (root.leaves() as unknown as { x0: number; y0: number; x1: number; y1: number; data: { c: Node } }[]).filter((l) => l.x1 - l.x0 > 2 && l.y1 - l.y0 > 2);
  }, [node, size]);

  const max = node.children[0]?.size || 1;
  return (
    <div className="treemap" ref={box} role="list" aria-label={node.name}>
      {cells.map((l, i) => {
        const n = l.data.c, w = l.x1 - l.x0, h = l.y1 - l.y0;
        const heat = Math.pow(n.size / max, 0.55);
        const open = n.is_dir && n.children.length > 0;
        return (
          <motion.button
            key={n.path + n.name}
            role="listitem"
            className={`cell${selected === n.path ? " on" : ""}${n.is_dir ? " dir" : ""}`}
            style={{ left: l.x0, top: l.y0, width: w, height: h, ["--heat" as string]: heat.toFixed(3) }}
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ delay: Math.min(i * 0.012, 0.25), duration: 0.28 }}
            onClick={() => onSelect(n)}
            onDoubleClick={() => open && onOpen(n)}
            title={`${n.name} · ${human(n.size)}`}
          >
            {w > 70 && h > 36 && (
              <>
                <span className="cell-name">{n.name}{n.is_dir ? "/" : ""}</span>
                <span className="cell-size">{human(n.size)}</span>
              </>
            )}
          </motion.button>
        );
      })}
    </div>
  );
}
