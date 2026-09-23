import { z } from "zod";

export const messageIdSchema = z.string().regex(/^[0-9A-HJKMNP-TV-Z]{26}$/i);

export const envelopeSchema = z.object({
  type: z.string(),
  requestId: z.string().optional(),
  timestamp: z.string().optional(),
  payload: z.record(z.string(), z.unknown()).optional(),
});

export const authSchema = envelopeSchema.extend({
  type: z.literal("auth"),
  payload: z.object({ deviceId: z.string().uuid(), token: z.string().min(40) }),
});

export const pairingRequestSchema = envelopeSchema.extend({
  type: z.literal("pairing_request"),
  payload: z.object({ code: z.string().regex(/^\d{6}$/) }),
});

export const pairingOkSchema = envelopeSchema.extend({
  type: z.literal("pairing_ok"),
  payload: z.object({ deviceId: z.string().uuid(), token: z.string() }),
});

export const syncRequestSchema = envelopeSchema.extend({
  type: z.literal("sync_request"),
  payload: z.object({ afterSeq: z.number().int().nonnegative() }),
});

export const chatMessageSchema = envelopeSchema.extend({
  type: z.literal("chat_message"),
  requestId: messageIdSchema,
  payload: z.object({ content: z.string().min(1).max(8192) }),
});

export const messageAckSchema = envelopeSchema.extend({
  type: z.literal("message_ack"),
  payload: z.object({ messageId: messageIdSchema, status: z.literal("stored"), seq: z.number().int().positive(), createdAt: z.string() }),
});

export const deliveredSchema = envelopeSchema.extend({
  type: z.literal("delivered"),
  payload: z.object({ messageId: messageIdSchema }),
});

export type Envelope = z.infer<typeof envelopeSchema>;
export type ChatMessageEnvelope = z.infer<typeof chatMessageSchema>;

export type ChatMessage = {
  seq?: number;
  id: string;
  sender: "windows" | "iphone";
  content: string;
  status: "pending" | "stored" | "delivered";
  createdAt: string;
};

export type Credentials = { deviceId: string; token: string };

export type ConnectionStatus = "connecting" | "connected" | "disconnected";

export function createEnvelope<T extends Record<string, unknown>>(
  type: string,
  payload: T,
  requestId?: string,
): Envelope {
  return {
    type,
    ...(requestId ? { requestId } : {}),
    timestamp: new Date().toISOString(),
    payload,
  };
}
