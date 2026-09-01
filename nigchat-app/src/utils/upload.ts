import * as ImagePicker from 'expo-image-picker';

import { media } from '../api/endpoints';

/**
 * Media upload.
 *
 * Three steps, and the middle one deliberately does not touch our API:
 *
 *   1. ask the backend for a signed URL
 *   2. PUT the bytes straight to storage
 *   3. tell the backend it finished
 *
 * If step 2 went through the API, a single 40 MB video on a mobile connection
 * would occupy a worker for its whole duration — and to the autoscaler that
 * worker looks idle, because it is blocked rather than busy.
 *
 * Step 3 matters more than it looks: an upload that never completes stays
 * `pending` and gets swept. Skipping it means paying to store an orphan.
 */

export interface UploadedMedia {
  id: string;
  url: string;
  mime_type: string;
  byte_size: number;
}

/**
 * Picks an image and uploads it.
 *
 * The picker is asked to compress and crop before anything leaves the device.
 * Uploading a 12-megapixel original to be displayed at 88 points wastes the
 * user's data — and on a Nigerian mobile plan that is money, not just time.
 */
export async function pickAndUploadImage(options: {
  purpose: 'avatar' | 'attachment';
  square?: boolean;
  onProgress?: (fraction: number) => void;
}): Promise<UploadedMedia | null> {
  const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
  if (!permission.granted) {
    throw new Error('NigChat needs permission to open your photos.');
  }

  const picked = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: ImagePicker.MediaTypeOptions.Images,
    allowsEditing: true,
    aspect: options.square ? [1, 1] : undefined,
    // 0.8 is the knee of the curve: most of the size saved, almost no visible
    // loss. Full quality on an avatar is pure waste.
    quality: 0.8,
    exif: false, // strips GPS coordinates from the photo
  });

  if (picked.canceled || !picked.assets?.length) return null;

  const asset = picked.assets[0];
  const mimeType = asset.mimeType ?? 'image/jpeg';

  // Read the real byte length rather than trusting the picker's metadata.
  const blob = await (await fetch(asset.uri)).blob();
  const byteSize = blob.size;

  options.onProgress?.(0.1);

  const ticket = await media.requestUpload({
    purpose: options.purpose,
    mime_type: mimeType,
    byte_size: byteSize,
    width: asset.width,
    height: asset.height,
  });

  options.onProgress?.(0.2);

  const headers: Record<string, string> = {};
  ticket.headers.forEach(([key, value]) => {
    headers[key] = value;
  });

  const response = await fetch(ticket.upload_url, {
    method: ticket.method,
    headers,
    body: blob,
  });

  if (!response.ok) {
    // The signed URL can expire mid-upload on a slow connection. Saying so is
    // more useful than "upload failed".
    throw new Error(
      response.status === 400
        ? 'The upload link expired. Please try again.'
        : 'Could not upload that image.',
    );
  }

  options.onProgress?.(0.9);

  const completed = await media.complete(ticket.media_id, byteSize);
  options.onProgress?.(1);

  return {
    id: completed.id,
    url: completed.url,
    mime_type: completed.mime_type,
    byte_size: completed.byte_size,
  };
}
