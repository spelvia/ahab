export function SidebarHeading({
	children,
	action,
}: {
	children: React.ReactNode;
	action?: React.ReactNode;
}) {
	return (
		<div className="flex items-center justify-between px-3 pt-3 pb-1.5">
			<h2 className="text-muted-foreground text-[0.6875rem] font-semibold tracking-wider uppercase">
				{children}
			</h2>
			{action}
		</div>
	);
}
