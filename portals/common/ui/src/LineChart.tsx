import { Box, Text, useComputedColorScheme, VisuallyHidden } from "@mantine/core";
import { useElementSize } from "@mantine/hooks";
import { type KeyboardEvent, type PointerEvent, useState } from "react";

/** One point: a time (ms since the epoch) and its value, or null when the source didn't say. */
export type LinePoint = { at: number; value: number | null };

export type LineChartProps = {
  /** What the chart shows, for screen readers and the data table, e.g. "PDP p99". */
  label: string;
  /** Oldest first. */
  points: LinePoint[];
  /** The time span to draw, in ms since the epoch. */
  from: number;
  to: number;
  /** Points further apart than this (ms) aren't joined: the gap is missing data. */
  maxGap: number;
  /** Formats a value for the axis and the readout, e.g. `(v) => `${v.toFixed(1)} ms``. */
  format: (value: number) => string;
  /** Formats a time for the readout (and the axis, unless `formatAxisTime` is given). */
  formatTime: (at: number) => string;
  /** Formats the axis's start, middle, and end, e.g. relative times ("24h ago", "now"). */
  formatAxisTime?: (at: number) => string;
  /** A fixed top of the scale, e.g. 100 for a percentage; otherwise it fits the data. */
  max?: number;
  height?: number;
};

// The series color: teal, one step darker than Mantine's light-mode primary so the line keeps
// 3:1 contrast against the surface in both schemes (checked with the dataviz palette validator).
const SERIES = { light: "var(--mantine-color-teal-7)", dark: "var(--mantine-color-teal-8)" };
const GRID = "var(--mantine-color-default-border)";
const SURFACE = "var(--mantine-color-body)";
const PAD = { top: 8, right: 8, bottom: 22, left: 52 };

/** A rounded top for the scale: 0.83 -> 1, 37 -> 40, 1300 -> 1500. */
function niceMax(v: number): number {
  if (v <= 0) return 1;
  const step = 10 ** Math.floor(Math.log10(v));
  const m = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10].find((f) => f * step >= v) ?? 10;
  return m * step;
}

/**
 * A single-series line chart over time, drawn in SVG: one axis from zero, gaps where data is
 * missing, and a crosshair readout on hover or keyboard focus (arrow keys move it). A hidden
 * table carries every value for screen readers.
 */
export function LineChart({
  label,
  points,
  from,
  to,
  maxGap,
  format,
  formatTime,
  formatAxisTime = formatTime,
  max,
  height = 140,
}: LineChartProps) {
  const { ref, width } = useElementSize();
  const scheme = useComputedColorScheme("light");
  const series = SERIES[scheme];
  const [active, setActive] = useState<number | null>(null);

  const shown = points.filter((p): p is { at: number; value: number } => p.value != null);
  const latest = shown.at(-1);
  if (!latest) {
    return (
      <Box h={height} style={{ display: "grid", placeItems: "center" }}>
        <Text size="sm" c="dimmed">
          Not reported
        </Text>
      </Box>
    );
  }

  const top = max ?? niceMax(Math.max(...shown.map((p) => p.value)));
  const plotW = Math.max(1, width - PAD.left - PAD.right);
  const plotH = height - PAD.top - PAD.bottom;
  const x = (at: number) => PAD.left + ((at - from) / (to - from)) * plotW;
  const y = (v: number) => PAD.top + plotH - (Math.min(v, top) / top) * plotH;

  // Runs of points close enough in time to join; a lone point is drawn as a dot.
  const runs: (typeof shown)[] = [];
  for (const p of shown) {
    const run = runs.at(-1);
    const last = run?.at(-1);
    if (run && last && p.at - last.at <= maxGap) run.push(p);
    else runs.push([p]);
  }
  const path = (run: typeof shown) => run.map((p, i) => `${i ? "L" : "M"}${x(p.at)},${y(p.value)}`).join("");

  const nearest = (px: number) => {
    let best = 0;
    shown.forEach((p, i) => {
      const current = shown.at(best);
      if (current && Math.abs(x(p.at) - px) < Math.abs(x(current.at) - px)) best = i;
    });
    return best;
  };
  const onPointer = (e: PointerEvent<SVGRectElement>) => {
    const box = e.currentTarget.ownerSVGElement?.getBoundingClientRect();
    if (box) setActive(nearest(e.clientX - box.left));
  };
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const step = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
    if (!step) return;
    e.preventDefault();
    setActive((a) => Math.min(shown.length - 1, Math.max(0, (a ?? shown.length - 1) + step)));
  };
  const point = active == null ? null : (shown.at(Math.min(active, shown.length - 1)) ?? null);
  const highest = Math.max(...shown.map((p) => p.value));
  const ticks = [0, top / 2, top];
  const times = [from, from + (to - from) / 2, to];

  return (
    <Box ref={ref} pos="relative" h={height}>
      {width > 0 && (
        <svg
          width={width}
          height={height}
          role="img"
          aria-label={`${label}, ${formatTime(from)} to ${formatTime(to)}. Latest ${format(latest.value)}, highest ${format(highest)}.`}
          tabIndex={0}
          onKeyDown={onKey}
          onFocus={() => setActive((a) => a ?? shown.length - 1)}
          onBlur={() => setActive(null)}
          style={{ display: "block", outline: "none" }}
        >
          {ticks.map((t) => (
            <g key={t}>
              <line x1={PAD.left} x2={PAD.left + plotW} y1={y(t)} y2={y(t)} stroke={GRID} strokeWidth={1} />
              <text
                x={PAD.left - 6}
                y={y(t)}
                textAnchor="end"
                dominantBaseline="middle"
                fontSize={11}
                fill="var(--mantine-color-dimmed)"
              >
                {format(t)}
              </text>
            </g>
          ))}
          {times.map((t, i) => (
            <text
              key={t}
              x={x(t)}
              y={height - 6}
              textAnchor={i === 0 ? "start" : i === times.length - 1 ? "end" : "middle"}
              fontSize={11}
              fill="var(--mantine-color-dimmed)"
            >
              {formatAxisTime(t)}
            </text>
          ))}
          {runs.map((run) => {
            const first = run.at(0);
            const last = run.at(-1);
            if (!first || !last) return null;
            return run.length > 1 ? (
              <g key={first.at}>
                <path
                  d={`${path(run)}L${x(last.at)},${y(0)}L${x(first.at)},${y(0)}Z`}
                  fill={series}
                  fillOpacity={0.1}
                />
                <path
                  d={path(run)}
                  fill="none"
                  stroke={series}
                  strokeWidth={2}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                />
              </g>
            ) : (
              <circle
                key={first.at}
                cx={x(first.at)}
                cy={y(first.value)}
                r={4}
                fill={series}
                stroke={SURFACE}
                strokeWidth={2}
              />
            );
          })}
          {point && (
            <g pointerEvents="none">
              <line x1={x(point.at)} x2={x(point.at)} y1={PAD.top} y2={PAD.top + plotH} stroke={GRID} strokeWidth={1} />
              <circle cx={x(point.at)} cy={y(point.value)} r={4} fill={series} stroke={SURFACE} strokeWidth={2} />
            </g>
          )}
          <rect
            x={PAD.left}
            y={0}
            width={plotW}
            height={height}
            fill="transparent"
            onPointerMove={onPointer}
            onPointerLeave={() => setActive(null)}
          />
        </svg>
      )}
      {point && (
        <Box
          pos="absolute"
          top={0}
          px={8}
          py={4}
          style={{
            left: Math.min(Math.max(x(point.at) + 8, 0), Math.max(0, width - 160)),
            pointerEvents: "none",
            background: "var(--mantine-color-body)",
            border: `1px solid ${GRID}`,
            borderRadius: 4,
            whiteSpace: "nowrap",
          }}
        >
          <Text size="sm" fw={600}>
            {format(point.value)}
          </Text>
          <Text size="xs" c="dimmed">
            {formatTime(point.at)}
          </Text>
        </Box>
      )}
      <VisuallyHidden>
        <table>
          <caption>{label}</caption>
          <tbody>
            {shown.map((p) => (
              <tr key={p.at}>
                <td>{formatTime(p.at)}</td>
                <td>{format(p.value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </VisuallyHidden>
    </Box>
  );
}
