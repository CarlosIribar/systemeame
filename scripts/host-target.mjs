export function hostTarget() {
  if (process.platform !== 'linux') return `${process.platform}-${process.arch}`;
  const libc = process.report?.getReport().header.glibcVersionRuntime ? 'gnu' : 'musl';
  return `linux-${process.arch}-${libc}`;
}
