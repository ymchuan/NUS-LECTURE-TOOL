export function requiredSlideConfirmations(
  currentDocumentId: number | null,
  currentPage: number | null,
  candidateDocumentId: number,
  candidatePage: number,
) {
  if (currentDocumentId === null || currentPage === null) return 2;
  if (currentDocumentId === candidateDocumentId && candidatePage - currentPage === 1) return 2;
  return 3;
}
