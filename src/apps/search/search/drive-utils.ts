export function driveLabelFromPath(path: string): string {
    const normalized = path.replace(/\//g, "\\");
    const match = normalized.match(/^[A-Za-z]:/);
    if (match) {
        return `${match[0].toUpperCase()}\\`;
    }
    return "Other";
}

export function animateNumber(
    prev: number,
    next: number,
    onUpdate: (value: number) => void,
    durationMs = 220,
): () => void {
    if (prev === next) {
        onUpdate(next);
        return () => {};
    }

    const from = Number.isFinite(prev) ? prev : 0;
    const to = Number.isFinite(next) ? next : 0;
    const delta = to - from;
    const duration = Math.max(80, durationMs);
    const startedAt = performance.now();
    let frame = 0;

    const tick = (now: number) => {
        const t = Math.min(1, (now - startedAt) / duration);
        const eased = 1 - Math.pow(1 - t, 3);
        onUpdate(Math.round(from + delta * eased));

        if (t < 1) {
            frame = requestAnimationFrame(tick);
        } else {
            onUpdate(to);
        }
    };

    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
}

export function stopDriveCountAnimations(
    cancels: Map<string, () => void>,
): void {
    for (const cancel of cancels.values()) {
        cancel();
    }
    cancels.clear();
}

export function animateDriveCount(
    drive: string,
    nextValue: number,
    displayedDriveScanCounts: Record<string, number>,
    cancels: Map<string, () => void>,
): void {
    cancels.get(drive)?.();
    const prevValue = displayedDriveScanCounts[drive] ?? 0;
    const cancel = animateNumber(prevValue, nextValue, (value) => {
        displayedDriveScanCounts[drive] = value;
    });
    cancels.set(drive, cancel);
}

export function initializeDriveScanSlots(rootsToScan: string[]): {
    driveScanOrder: string[];
    driveScanCounts: Record<string, number>;
    displayedDriveScanCounts: Record<string, number>;
} {
    const drives = Array.from(new Set(rootsToScan.map(driveLabelFromPath))).slice(
        0,
        4,
    );

    return {
        driveScanOrder: drives,
        driveScanCounts: Object.fromEntries(drives.map((drive) => [drive, 0])),
        displayedDriveScanCounts: Object.fromEntries(
            drives.map((drive) => [drive, 0]),
        ),
    };
}

export function incrementDriveScanned(
    rootPath: string,
    scannedFolders: number,
    driveScanOrder: string[],
    driveScanCounts: Record<string, number>,
    displayedDriveScanCounts: Record<string, number>,
    cancels: Map<string, () => void>,
): string[] {
    if (scannedFolders <= 0) return driveScanOrder;

    const drive = driveLabelFromPath(rootPath);
    let nextOrder = driveScanOrder;

    if (!nextOrder.includes(drive) && nextOrder.length < 4) {
        nextOrder = [...nextOrder, drive];
        displayedDriveScanCounts[drive] = displayedDriveScanCounts[drive] ?? 0;
    }

    driveScanCounts[drive] = (driveScanCounts[drive] ?? 0) + scannedFolders;
    animateDriveCount(drive, driveScanCounts[drive], displayedDriveScanCounts, cancels);

    return nextOrder;
}
