/**
 * Gateway API — pairing and management
 */

import { apiPost } from "./client.js";

export interface PairingRequest {
  code: string;
  workspaceId?: string;
}

export interface PairingResponse {
  thingId: string;
  thingName: string;
  hostname: string;
  ip: string;
}

export async function pairGateway(req: PairingRequest): Promise<PairingResponse> {
  const res = await apiPost<PairingResponse>("/gateway/pair", req);
  return res.result as PairingResponse;
}
