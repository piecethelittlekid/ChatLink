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
  payload: z.object({ code: z.string().regex(/^\d{6}$/) }),
});

export const chatMessageSchema = envelopeSchema.extend({
  type: z.literal("chat_message"),
  requestId: messageIdSchema,
  payload: z.object({ content: z.string().min(1).max(8192) }),
});

export const messageAckSchema = envelopeSchema.extend({
  type: z.literal("message_ack"),
  payload: z.object({ messageId: messageIdSchema, status: z.literal("stored") }),
});

export const deliveredSchema = envelopeSchema.extend({
  type: z.literal("delivered"),
  payload: z.object({ messageId: messageIdSchema }),
});

export type Envelope = z.infer<typeof envelopeSchema>;
export type ChatMessageEnvelope = z.infer<typeof chatMessageSchema>;

export type ChatMessage = {
  id: string;
  sender: "windows" | "iphone";
  content: string;
  status: "pending" | "stored" | "delivered";
  createdAt: string;
};

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
