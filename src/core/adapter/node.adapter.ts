import { node } from '@elysiajs/node';

/**
 * Singleton Node.js adapter instance for Elysia.
 *
 * Sharing a single adapter instance is strictly required across root app and all
 * sub-routes/plugins utilizing WebSockets, because `@elysiajs/node` binds its
 * WebSocket context store internally to the adapter instance.
 */
export const nodeAdapter = node();
