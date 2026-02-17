#!/usr/bin/env node
/**
 * HomunBot WhatsApp Bridge
 *
 * Connects WhatsApp via Baileys to the Rust agent via WebSocket.
 * Handles authentication, message forwarding, and reconnection.
 *
 * Usage:
 *   npm run build && npm start
 *
 * Environment variables:
 *   BRIDGE_PORT  — WebSocket port (default: 3001)
 *   AUTH_DIR     — Baileys credential directory (default: ~/.homunbot/whatsapp-auth)
 *   BRIDGE_TOKEN — Optional auth token for WebSocket connections
 */

// Polyfill crypto for Baileys in ESM
import { webcrypto } from 'crypto';
if (!globalThis.crypto) {
  (globalThis as any).crypto = webcrypto;
}

import { BridgeServer } from './server.js';
import { homedir } from 'os';
import { join } from 'path';

const PORT = parseInt(process.env.BRIDGE_PORT || '3001', 10);
const AUTH_DIR = process.env.AUTH_DIR || join(homedir(), '.homunbot', 'whatsapp-auth');
const TOKEN = process.env.BRIDGE_TOKEN || undefined;

console.log('HomunBot WhatsApp Bridge');
console.log('========================\n');

const server = new BridgeServer(PORT, AUTH_DIR, TOKEN);

// Handle graceful shutdown
process.on('SIGINT', async () => {
  console.log('\n\nShutting down...');
  await server.stop();
  process.exit(0);
});

process.on('SIGTERM', async () => {
  await server.stop();
  process.exit(0);
});

// Start the server
server.start().catch((error) => {
  console.error('Failed to start bridge:', error);
  process.exit(1);
});
