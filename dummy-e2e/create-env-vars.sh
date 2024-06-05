# This script creates a .env file with the necessary environment variables
# for the local environment to run the tests.
# Needs to be executed within the same directory as the script.
ENV_FILE=${ENV_OUTPUT_FILE:-$PWD/.env}
echo "Creating .env file at $ENV_FILE"

II_CANISTER_ID=$(dfx canister id internet_identity --network local)
ISSUER_CANISTER_ID=$(dfx canister id dummy_issuer --network local)
RP_CANISTER_ID=$(dfx canister id dummy_relying_party --network local)

REPLICA_SERVER_PORT=$(dfx info webserver-port)
II_URL="http://${II_CANISTER_ID}.localhost:${REPLICA_SERVER_PORT}"
ISSUER_ORIGIN="http://${ISSUER_CANISTER_ID}.localhost:${REPLICA_SERVER_PORT}"
RP_ORIGIN="http://${RP_CANISTER_ID}.localhost:${REPLICA_SERVER_PORT}"
echo "II_URL=${II_URL}" > $ENV_FILE
echo "ISSUER_URL=${ISSUER_ORIGIN}" >> $ENV_FILE
echo "RP_URL=${RP_ORIGIN}" >> $ENV_FILE