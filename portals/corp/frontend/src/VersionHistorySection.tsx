import { EmptyState, Section, ShowMore } from "@awzy/ui";
import { Group, Table } from "@mantine/core";
import { IconHistory } from "@tabler/icons-react";
import { graphql, usePaginationFragment } from "react-relay";

import type { VersionHistorySection_install$key } from "./__generated__/VersionHistorySection_install.graphql";
import type { VersionHistorySectionPaginationQuery } from "./__generated__/VersionHistorySectionPaginationQuery.graphql";
import { formatTime } from "./format";

/** The chart versions an install has run, newest first: the last few, and more on request. */
export function VersionHistorySection({ install }: { install: VersionHistorySection_install$key }) {
  const { data, loadNext, hasNext, isLoadingNext } = usePaginationFragment<
    VersionHistorySectionPaginationQuery,
    VersionHistorySection_install$key
  >(
    graphql`
      fragment VersionHistorySection_install on Install
      @argumentDefinitions(first: { type: "Int", defaultValue: 5 }, after: { type: "String" })
      @refetchable(queryName: "VersionHistorySectionPaginationQuery") {
        versionHistory(first: $first, after: $after) @connection(key: "VersionHistorySection_versionHistory") {
          edges {
            node {
              chartVersion
              firstSeenAt
            }
          }
        }
      }
    `,
    install,
  );
  const history = data.versionHistory.edges.map((e) => e.node);
  return (
    <Section title="Version history" description="Each chart version the install reported, newest first." flush>
      {history.length === 0 ? (
        <EmptyState icon={<IconHistory size={22} />} title="No chart version reported yet" />
      ) : (
        <>
          <Table>
            <Table.Tbody>
              {history.map((v) => (
                <Table.Tr key={v.chartVersion}>
                  <Table.Td ff="monospace" fz="sm">
                    {v.chartVersion}
                  </Table.Td>
                  <Table.Td c="dimmed" fz="sm">
                    <Group gap={4}>First seen {formatTime(v.firstSeenAt)}</Group>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
          <ShowMore hasMore={hasNext} loading={isLoadingNext} onMore={() => loadNext(20)} />
        </>
      )}
    </Section>
  );
}
