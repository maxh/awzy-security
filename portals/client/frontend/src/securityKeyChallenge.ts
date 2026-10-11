import { commitMutation, graphql } from "react-relay";
import type { Environment, GraphQLTaggedNode, MutationParameters } from "relay-runtime";

import type { securityKeyChallengeBeginMutation } from "./__generated__/securityKeyChallengeBeginMutation.graphql";
import type { securityKeyChallengeFinishMutation } from "./__generated__/securityKeyChallengeFinishMutation.graphql";
import { getAssertion } from "./webauthn";

const begin = graphql`
  mutation securityKeyChallengeBeginMutation($input: BeginChallengeVerificationInput!) {
    beginChallengeVerification(input: $input) {
      ceremonyQid
      optionsJson
    }
  }
`;

const finish = graphql`
  mutation securityKeyChallengeFinishMutation($input: FinishChallengeVerificationInput!) {
    finishChallengeVerification(input: $input) {
      challenge {
        id
        status
      }
    }
  }
`;

/** A mutation as a promise, rejected with the server's first error. */
export function mutate<T extends MutationParameters>(
  environment: Environment,
  mutation: GraphQLTaggedNode,
  variables: T["variables"],
): Promise<T["response"]> {
  return new Promise((resolve, reject) =>
    commitMutation<T>(environment, {
      mutation,
      variables,
      onCompleted: (response, errors) => {
        const first = errors?.[0];
        if (first) reject(new Error(first.message));
        else resolve(response);
      },
      onError: reject,
    }),
  );
}

/**
 * Passes a security key challenge with a touch of the viewer's key, as changing what's enforced
 * asks for under the base policies. The change is then made again.
 */
export async function passChallenge(environment: Environment, challengeQid: string): Promise<void> {
  const { beginChallengeVerification: v } = await mutate<securityKeyChallengeBeginMutation>(environment, begin, {
    input: { challengeQid },
  });
  const assertion = await getAssertion(v.ceremonyQid, v.optionsJson);
  await mutate<securityKeyChallengeFinishMutation>(environment, finish, { input: { assertion } });
}
