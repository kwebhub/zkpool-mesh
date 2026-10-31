//! Generate a TypeScript client from the Anchor IDL using Codama.
//!
//! Input:  ../onchain/target/idl/zk_pool.json
//! Output: ./src/generated/zk_pool/
//!
//! Run:    pnpm generate:client

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createFromRoot } from "codama";
import { rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const webDir = join(__dirname, "..");
const idlPath = join(webDir, "..", "onchain", "target", "idl", "zk_pool.json");
const outputDir = join(webDir, "src", "generated", "zk_pool");

const idl = JSON.parse(readFileSync(idlPath, "utf8"));
const codama = createFromRoot(rootNodeFromAnchor(idl));

codama.accept(renderVisitor(outputDir));

console.log(`✓ Generated client → ${outputDir}`);
