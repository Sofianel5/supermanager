export function displayEmployeeName(value: string) {
  const trimmed = value.trim();
  return trimmed || "Unknown member";
}
