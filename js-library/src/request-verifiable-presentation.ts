export type CredentialsSpec = {
  type: string;
  arguments: Record<string, string | number>;
};

export const requestVerifiablePresentation = () => {
  console.log("Hello, world");
  return 2;
};
