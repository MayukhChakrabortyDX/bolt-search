import { DIR_INDENT_CLASSES, FILE_INDENT_CLASSES } from "./constants";
import type {
    FileEntry,
    MutableTreeNode,
    TreeNode,
    TreeRow,
} from "./types";

export function normalizePath(path: string): string {
    return path.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/$/, "");
}

export function displayPath(path: string): string {
    return path.replace(/\//g, "\\");
}

export function driveLabelFromPath(path: string): string {
    const normalized = path.replace(/\//g, "\\");
    const match = normalized.match(/^[A-Za-z]:/);
    if (match) {
        return `${match[0].toUpperCase()}\\`;
    }
    return "Other";
}

function createMutableNode(
    name: string,
    path: string,
    isDir: boolean,
): MutableTreeNode {
    return { name, path, isDir, children: new Map() };
}

function insertPathIntoTree(
    roots: Map<string, MutableTreeNode>,
    normalizedPath: string,
    isLeafDirectory: boolean,
) {
    const segments = normalizedPath.split("/").filter(Boolean);
    if (segments.length === 0) return;

    let currentMap = roots;
    let currentPath = "";

    for (let i = 0; i < segments.length; i++) {
        const segment = segments[i];
        const isLast = i === segments.length - 1;
        currentPath = currentPath ? `${currentPath}/${segment}` : segment;

        let node = currentMap.get(segment);
        if (!node) {
            node = createMutableNode(
                segment,
                currentPath,
                isLast ? isLeafDirectory : true,
            );
            currentMap.set(segment, node);
        } else if (!isLast || isLeafDirectory) {
            node.isDir = true;
        }

        currentMap = node.children;
    }
}

function mutableToTree(node: MutableTreeNode): TreeNode {
    const children = Array.from(node.children.values())
        .map(mutableToTree)
        .sort((a, b) => {
            if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
            return a.name.localeCompare(b.name);
        });

    return {
        name: node.name,
        path: node.path,
        isDir: node.isDir || children.length > 0,
        children,
    };
}

export function buildResultTree(
    entries: FileEntry[],
    inFlightFolders: string[],
): TreeNode[] {
    const roots = new Map<string, MutableTreeNode>();

    for (const entry of entries) {
        const normalized = normalizePath(entry.path);
        if (!normalized) continue;
        insertPathIntoTree(roots, normalized, entry.is_dir);
    }

    for (const folder of inFlightFolders) {
        const normalized = normalizePath(folder);
        if (!normalized) continue;
        insertPathIntoTree(roots, normalized, true);
    }

    return Array.from(roots.values())
        .map(mutableToTree)
        .sort((a, b) => {
            if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
            return a.name.localeCompare(b.name);
        });
}

export function flattenVisibleRows(
    nodes: TreeNode[],
    isDirectoryOpen: (path: string, depth: number) => boolean,
    depth = 0,
    rows: TreeRow[] = [],
): TreeRow[] {
    for (const node of nodes) {
        const hasChildren = node.children.length > 0;
        const isOpen =
            node.isDir && hasChildren ? isDirectoryOpen(node.path, depth) : false;

        rows.push({ node, depth, hasChildren, isOpen });

        if (node.isDir && hasChildren && isOpen) {
            flattenVisibleRows(node.children, isDirectoryOpen, depth + 1, rows);
        }
    }

    return rows;
}

export function rowIndentClass(depth: number, kind: "dir" | "file"): string {
    const classes = kind === "dir" ? DIR_INDENT_CLASSES : FILE_INDENT_CLASSES;
    const index = Math.max(0, Math.min(depth, classes.length - 1));
    return classes[index];
}
