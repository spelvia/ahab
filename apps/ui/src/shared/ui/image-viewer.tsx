import { cn } from "cn";
import { ExternalLink, Maximize, X, ZoomIn, ZoomOut } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import { useCallback, useEffect, useRef, useState } from "react";

import { formatNumber, useTranslation } from "@/shared/i18n";

/** How far the image may shrink or grow, against its size fitted to the window. */
const MIN_SCALE = 0.25;
const MAX_SCALE = 16;
/** One click of the + or − button, or one key press. */
const STEP = 1.25;

interface View {
	scale: number;
	x: number;
	y: number;
}

const FITTED: View = { scale: 1, x: 0, y: 0 };

function clamp(scale: number): number {
	return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/**
 * `view` zoomed to `scale` around `point` (from the stage's centre), so the
 * pixel under the cursor stays under it.
 */
function zoomAround(view: View, scale: number, point = { x: 0, y: 0 }): View {
	const next = clamp(scale);
	if (next === 1 && view.scale !== 1 && scale <= 1) return FITTED;
	const ratio = next / view.scale;
	return {
		scale: next,
		x: point.x - (point.x - view.x) * ratio,
		y: point.y - (point.y - view.y) * ratio,
	};
}

/**
 * An image over the whole window, to look at closely: the wheel, a pinch or
 * + and − zoom (around the cursor), dragging moves it, a double click
 * switches between fitted and actual size, 0 fits it again. Escape or ✕
 * closes it. It opens fitted; mount it when opened (`{open && <ImageViewer …/>}`)
 * for each opening to start fitted again.
 */
export function ImageViewer({
	src,
	alt,
	open,
	onOpenChange,
}: {
	src: string;
	alt?: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation();
	const [view, setView] = useState<View>(FITTED);
	const [dragging, setDragging] = useState(false);
	const stage = useRef<HTMLDivElement>(null);
	const image = useRef<HTMLImageElement>(null);
	const pointers = useRef(new Map<number, { x: number; y: number }>());
	const pinch = useRef<number | undefined>(undefined);
	/** Whether the press started on the dark area and has not moved: a click there closes. */
	const backdropPress = useRef(false);
	const pressStart = useRef({ x: 0, y: 0 });
	/** The fitted image's size against its own: 0.5 when fitted shows it at half size. */
	const [fitRatio, setFitRatio] = useState(1);

	const measure = useCallback(() => {
		const element = image.current;
		if (element?.naturalWidth && element.offsetWidth)
			setFitRatio(element.offsetWidth / element.naturalWidth);
	}, []);

	useEffect(() => {
		const element = stage.current;
		if (!open || !element) return;
		const observer = new ResizeObserver(measure);
		observer.observe(element);
		return () => observer.disconnect();
	}, [open, measure]);

	/** A point on the screen, from the stage's centre. */
	const fromCentre = useCallback((clientX: number, clientY: number) => {
		const box = stage.current?.getBoundingClientRect();
		if (!box) return { x: 0, y: 0 };
		return { x: clientX - box.left - box.width / 2, y: clientY - box.top - box.height / 2 };
	}, []);

	// The wheel zooms instead of scrolling the page behind: a non-passive listener.
	useEffect(() => {
		const element = stage.current;
		if (!open || !element) return;
		const onWheel = (event: WheelEvent) => {
			event.preventDefault();
			const factor = Math.exp(-event.deltaY * (event.deltaMode === 1 ? 0.05 : 0.0015));
			const point = fromCentre(event.clientX, event.clientY);
			setView((current) => zoomAround(current, current.scale * factor, point));
		};
		element.addEventListener("wheel", onWheel, { passive: false });
		return () => element.removeEventListener("wheel", onWheel);
	}, [open, fromCentre]);

	const zoomBy = (factor: number) =>
		setView((current) => zoomAround(current, current.scale * factor));
	const toggleActual = (point?: { x: number; y: number }) =>
		setView((current) => {
			// The scale at which one image pixel is one screen pixel.
			const actual = 1 / fitRatio;
			return Math.abs(current.scale - 1) < 0.01 && Math.abs(actual - 1) >= 0.01
				? zoomAround(current, actual, point)
				: FITTED;
		});

	const percent = Math.round(view.scale * fitRatio * 100);

	return (
		<Primitive.Root open={open} onOpenChange={onOpenChange}>
			<Primitive.Portal>
				<Primitive.Overlay className="data-[state=open]:animate-in data-[state=open]:fade-in-0 fixed inset-0 z-40 bg-black/85 backdrop-blur-[2px]" />
				<Primitive.Content
					aria-describedby={undefined}
					className="data-[state=open]:animate-in data-[state=open]:fade-in-0 fixed inset-0 z-50 flex flex-col outline-none"
					// React events bubble through the portal: keep them from the
					// message or row the image sits in.
					onClick={(event) => event.stopPropagation()}
					onDoubleClick={(event) => event.stopPropagation()}
					onPointerDown={(event) => event.stopPropagation()}
					onKeyDown={(event) => {
						if (event.key === "+" || event.key === "=") zoomBy(STEP);
						else if (event.key === "-" || event.key === "_") zoomBy(1 / STEP);
						else if (event.key === "0") setView(FITTED);
						else return;
						event.preventDefault();
					}}
				>
					<Primitive.Title className="sr-only">{alt || t("imageViewer.title")}</Primitive.Title>
					<div
						ref={stage}
						className={cn(
							"relative flex min-h-0 flex-1 touch-none items-center justify-center overflow-hidden p-4 select-none sm:p-10",
							view.scale > 1 ? (dragging ? "cursor-grabbing" : "cursor-grab") : "cursor-zoom-in",
						)}
						onClick={() => {
							// A click on the dark area around a fitted image closes it.
							if (backdropPress.current && view.scale <= 1) onOpenChange(false);
						}}
						onDoubleClick={(event) => toggleActual(fromCentre(event.clientX, event.clientY))}
						onPointerDown={(event) => {
							if (event.button !== 0) return;
							backdropPress.current = event.target === event.currentTarget;
							pressStart.current = { x: event.clientX, y: event.clientY };
							event.currentTarget.setPointerCapture(event.pointerId);
							pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
							pinch.current = undefined;
							setDragging(true);
						}}
						onPointerMove={(event) => {
							const last = pointers.current.get(event.pointerId);
							if (!last) return;
							const now = { x: event.clientX, y: event.clientY };
							if (Math.hypot(now.x - pressStart.current.x, now.y - pressStart.current.y) > 4)
								backdropPress.current = false;
							pointers.current.set(event.pointerId, now);
							if (pointers.current.size === 2) {
								const [a, b] = [...pointers.current.values()];
								if (!a || !b) return;
								const distance = Math.hypot(a.x - b.x, a.y - b.y);
								const previous = pinch.current;
								pinch.current = distance;
								if (previous) {
									const point = fromCentre((a.x + b.x) / 2, (a.y + b.y) / 2);
									setView((current) =>
										zoomAround(current, (current.scale * distance) / previous, point),
									);
								}
								return;
							}
							setView((current) =>
								current.scale === 1 && current.x === 0 && current.y === 0
									? current
									: { ...current, x: current.x + now.x - last.x, y: current.y + now.y - last.y },
							);
						}}
						onPointerUp={(event) => {
							pointers.current.delete(event.pointerId);
							pinch.current = undefined;
							if (pointers.current.size === 0) setDragging(false);
						}}
						onPointerCancel={(event) => {
							pointers.current.delete(event.pointerId);
							pinch.current = undefined;
							if (pointers.current.size === 0) setDragging(false);
						}}
					>
						<img
							ref={image}
							src={src}
							alt={alt ?? ""}
							draggable={false}
							className={cn(
								"max-h-full max-w-full object-contain shadow-[0_24px_64px_rgba(0,0,0,0.5)]",
								!dragging && "transition-transform duration-100 ease-out",
							)}
							style={{
								transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
							}}
							onLoad={measure}
						/>
					</div>
					<div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">
						<div className="bg-popover/95 text-popover-foreground border-border pointer-events-auto flex items-center gap-1 rounded-lg border p-1 shadow-lg">
							<ViewerButton
								label={t("imageViewer.zoomOut")}
								onClick={() => zoomBy(1 / STEP)}
								disabled={view.scale <= MIN_SCALE}
							>
								<ZoomOut />
							</ViewerButton>
							<button
								type="button"
								title={t("imageViewer.actualSize")}
								onClick={() => toggleActual()}
								className="hover:bg-accent focus-visible:ring-ring min-w-14 rounded-md px-2 py-1.5 text-center text-xs tabular-nums outline-none focus-visible:ring-2"
							>
								{t("imageViewer.zoom", { value: formatNumber(percent) })}
							</button>
							<ViewerButton
								label={t("imageViewer.zoomIn")}
								onClick={() => zoomBy(STEP)}
								disabled={view.scale >= MAX_SCALE}
							>
								<ZoomIn />
							</ViewerButton>
							<ViewerButton label={t("imageViewer.fit")} onClick={() => setView(FITTED)}>
								<Maximize />
							</ViewerButton>
							<a
								href={src}
								target="_blank"
								rel="noreferrer"
								aria-label={t("imageViewer.openOriginal")}
								title={t("imageViewer.openOriginal")}
								className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-8 place-items-center rounded-md outline-none focus-visible:ring-2 [&_svg]:size-4"
							>
								<ExternalLink />
							</a>
							<Primitive.Close
								aria-label={t("actions.close")}
								title={t("actions.close")}
								className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-8 place-items-center rounded-md outline-none focus-visible:ring-2 [&_svg]:size-4"
							>
								<X />
							</Primitive.Close>
						</div>
					</div>
				</Primitive.Content>
			</Primitive.Portal>
		</Primitive.Root>
	);
}

function ViewerButton({
	label,
	onClick,
	disabled,
	children,
}: {
	label: string;
	onClick: () => void;
	disabled?: boolean;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			disabled={disabled}
			onClick={onClick}
			className="text-muted-foreground hover:text-foreground hover:bg-accent focus-visible:ring-ring grid size-8 place-items-center rounded-md outline-none focus-visible:ring-2 disabled:opacity-40 [&_svg]:size-4"
		>
			{children}
		</button>
	);
}
