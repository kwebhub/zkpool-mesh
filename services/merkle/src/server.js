//! Entry point: start the Merkle service HTTP server.
//!
//! Business logic lives in `src/app.js` (buildApp). This file only handles
//! process-level concerns: port binding and shutdown.

import { buildApp } from "./app.js";

const PORT = Number(process.env.PORT ?? 4003);
const HOST = process.env.HOST ?? "0.0.0.0";

const app = await buildApp();

try {
  await app.listen({ port: PORT, host: HOST });
} catch (err) {
  app.log.error(err);
  process.exit(1);
}
