"use client";
import {
  Accordion,
  Button,
  ButtonGroup,
  Chip,
  Dropdown,
  Table,
} from "@heroui/react";
import { useState } from "react";
import ApplicationDeleteModal from "./ApplicationDeleteModal";
import ApplicationGiftModal from "./ApplicationGiftModal";
import ApplicationCommercialModal from "./ApplicationCommercialModal";
import ApplicationCommerceModal from "./ApplicationCommerceModal";
import ApplicationImportModal from "./ApplicationImportModal";
import ApplicationSubmissionsModal from "./ApplicationSubmissionsModal";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { OFFICIAL_PUBLISHER } from "../../../lib/application-publisher";
import { formatDate } from "../../../lib/format";
import {
  operationData,
  operationCommand as operationJson,
} from "../../../lib/operation-api";
import type {
  ApplicationProduct,
  ApplicationReleaseSubmission,
  Customer,
} from "../../../lib/operation-types";

export default function ApplicationProductsPage() {
  const {
    data: result,
    error: queryError,
    loading,
    refresh,
  } = useData<ApplicationProduct[]>("/api/application-products");
  const data = result ?? [];
  const {
    data: customers,
    loading: customersLoading,
    error: customersError,
  } = useData<Customer[]>("/api/customers");
  const customerNames = new Map(
    (customers ?? []).map((customer) => [customer.customerId, customer.name]),
  );
  customerNames.set(
    OFFICIAL_PUBLISHER.subject,
    `${OFFICIAL_PUBLISHER.name}（官方）`,
  );
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [commercialProduct, setCommercialProduct] =
    useState<ApplicationProduct | null>(null);
  const [commerceOpen, setCommerceOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [submissionsOpen, setSubmissionsOpen] = useState(false);
  const [deleting, setDeleting] = useState<ApplicationProduct | null>(null);
  const [selected, setSelected] = useState<ApplicationProduct | null>(null);
  const [versionProduct, setVersionProduct] =
    useState<ApplicationProduct | null>(null);
  const [releases, setReleases] = useState<ApplicationReleaseSubmission[]>([]);
  async function toggleListing(product: ApplicationProduct) {
    if (product.listingStatus !== "listed") {
      await chooseVersion(product);
      return;
    }
    await mutation.run(
      () =>
        operationJson(
          `/api/application-products/${encodeURIComponent(product.productId)}/listing`,
          { listed: product.listingStatus !== "listed" },
        ),
      refresh,
    );
  }
  async function chooseVersion(product: ApplicationProduct) {
    await mutation.run(
      () =>
        operationData<ApplicationReleaseSubmission[]>(
          `/api/application-products/${encodeURIComponent(product.productId)}/releases`,
        ),
      (items) => {
        setReleases(items);
        setVersionProduct(product);
      },
    );
  }
  async function listVersion(releaseId: string) {
    if (!versionProduct) return;
    await mutation.run(
      () =>
        operationJson(
          `/api/application-products/${encodeURIComponent(versionProduct.productId)}/listing`,
          { listed: true, releaseId },
        ),
      () => {
        setVersionProduct(null);
        refresh();
      },
    );
  }
  return (
    <>
      {commercialProduct && (
        <ApplicationCommercialModal
          product={commercialProduct}
          onClose={() => setCommercialProduct(null)}
          onChanged={refresh}
        />
      )}
      {commerceOpen && (
        <ApplicationCommerceModal onClose={() => setCommerceOpen(false)} />
      )}
      {importOpen && (
        <ApplicationImportModal
          customers={customers ?? []}
          products={data}
          onClose={() => setImportOpen(false)}
          onImported={refresh}
        />
      )}
      <OperationPageHeader
        eyebrow="APPLICATION PRODUCT"
        title="应用管理"
        description="审核应用、售价与分成，上架后由客户自主领取或购买，自动发放整包权益。"
        loading={loading || mutation.pending}
        onRefresh={refresh}
        actions={
          <>
            <Button onPress={() => setCommerceOpen(true)}>交易与分成</Button>
            <Button variant="secondary" onPress={() => setImportOpen(true)}>
              导入应用包
            </Button>
            <Button onPress={() => setSubmissionsOpen(true)}>
              应用上线申请管理
            </Button>
          </>
        }
      />
      {error ? <p className="mb-4 text-sm text-danger">{error}</p> : null}
      {customersError ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          客户名称加载失败，暂时仅显示客户 ID：{customersError}
        </p>
      ) : null}
      <Table>
        <Table.ScrollContainer>
          <Table.Content
            aria-label="应用商品"
            className="min-w-[1640px] table-fixed"
          >
            <Table.Header>
              <Table.Column isRowHeader id="application" className="w-[220px]">
                应用
              </Table.Column>
              <Table.Column id="version" className="w-[140px]">
                版本
              </Table.Column>
              <Table.Column id="pricing" className="w-[200px]">
                费用
              </Table.Column>
              <Table.Column id="type" className="w-[300px]">
                应用类型
              </Table.Column>
              <Table.Column id="publisher" className="w-[300px]">
                发布客户
              </Table.Column>
              <Table.Column id="status" className="w-[120px]">
                上架状态
              </Table.Column>
              <Table.Column id="entitlements" className="w-[140px]">
                权益客户
              </Table.Column>
              <Table.Column id="actions" className="w-[180px]">
                操作
              </Table.Column>
            </Table.Header>
            <Table.Body
              renderEmptyState={() => (
                <p>
                  {loading ? "正在加载应用商品…" : "暂无已审核通过的应用商品。"}
                </p>
              )}
            >
              {data.map((item) => (
                <Table.Row key={item.productId} id={item.productId}>
                  <Table.Cell className="align-middle break-words">
                    <strong>{item.name}</strong>
                  </Table.Cell>
                  <Table.Cell className="align-middle break-words">
                    <p>{item.version ? `v${item.version}` : "暂无版本"}</p>
                    <p className="text-xs text-muted">
                      待审核 {item.pendingCount ?? 0}
                    </p>
                  </Table.Cell>
                  <Table.Cell className="align-middle">
                    {item.commercialReady ? (
                      <>
                        <p>
                          {item.pricingMode === "free"
                            ? "免费领取"
                            : `¥${((item.priceCents ?? 0) / 100).toFixed(2)}`}
                        </p>
                        {(item.publisherRateBasisPoints ?? 0) > 0 ? (
                          <p className="text-xs text-muted">
                            发布者分成{" "}
                            {(
                              (item.publisherRateBasisPoints ?? 0) / 100
                            ).toFixed(2)}
                            %
                          </p>
                        ) : null}
                      </>
                    ) : (
                      <p className="text-muted">商业资料待完善</p>
                    )}
                  </Table.Cell>
                  <Table.Cell className="align-middle">
                    <Accordion>
                      <Accordion.Item>
                        <Accordion.Heading>
                          <Accordion.Trigger
                            className="gap-2"
                            aria-label={`${item.name}：展开成员与依赖`}
                          >
                            <Chip size="sm" variant="primary" color="accent">
                              {item.applicationType === "suite"
                                ? "套应用"
                                : "单应用"}
                            </Chip>
                            {item.applicationType === "suite" ? (
                              <span className="text-xs text-muted">
                                {item.members?.length ?? 0} 个成员
                              </span>
                            ) : null}
                            <Accordion.Indicator />
                          </Accordion.Trigger>
                        </Accordion.Heading>
                        <Accordion.Panel>
                          <Accordion.Body>
                            <div className="break-all">
                              {item.members?.map((member) => (
                                <p key={member.appId}>
                                  {member.name}（{member.appId}）
                                </p>
                              ))}
                              {item.dependencies?.map((dependency, index) => (
                                <p key={index}>
                                  {dependency.sourceAppId} →{" "}
                                  {dependency.targetAppId}：{dependency.path}
                                </p>
                              ))}
                              {!item.members?.length &&
                              !item.dependencies?.length ? (
                                <p>暂无成员与依赖资料。</p>
                              ) : null}
                            </div>
                          </Accordion.Body>
                        </Accordion.Panel>
                      </Accordion.Item>
                    </Accordion>
                  </Table.Cell>
                  <Table.Cell className="align-middle break-all">
                    <p>
                      {customerNames.get(item.publisherSubject) ||
                        (customersLoading
                          ? "正在加载客户…"
                          : customersError
                            ? "客户名称暂不可用"
                            : "未关联客户")}
                    </p>
                    <p className="text-xs text-muted">
                      {item.publisherSubject || "客户 ID 缺失"}
                    </p>
                  </Table.Cell>
                  <Table.Cell className="align-middle">
                    <Chip
                      size="sm"
                      variant="soft"
                      color={
                        item.listingStatus === "listed" ? "success" : "default"
                      }
                    >
                      {item.listingStatus === "listed"
                        ? "已上架"
                        : item.listingStatus === "withdrawn"
                          ? "已下架"
                          : "状态未知"}
                    </Chip>
                  </Table.Cell>
                  <Table.Cell className="align-middle">
                    {item.entitlementCount ?? 0}
                  </Table.Cell>
                  <Table.Cell className="align-middle">
                    <ButtonGroup size="sm" isDisabled={mutation.pending}>
                      <Button onPress={() => void toggleListing(item)}>
                        {item.listingStatus === "listed" ? "下架" : "上架"}
                      </Button>
                      <Dropdown>
                        <Button
                          variant="secondary"
                          aria-label={`${item.name}的更多操作`}
                        >
                          更多
                        </Button>
                        <Dropdown.Popover placement="bottom end">
                          <Dropdown.Menu aria-label={`${item.name}的更多操作`}>
                            <Dropdown.Item
                              id="commercial"
                              onAction={() => setCommercialProduct(item)}
                            >
                              销售配置
                            </Dropdown.Item>
                            {item.listingStatus === "listed" ? (
                              <Dropdown.Item
                                id="version"
                                onAction={() => void chooseVersion(item)}
                              >
                                切换版本
                              </Dropdown.Item>
                            ) : null}
                            <Dropdown.Item
                              id="gift"
                              onAction={() => setSelected(item)}
                            >
                              赠送
                            </Dropdown.Item>
                            <Dropdown.Item
                              id="delete"
                              variant="danger"
                              isDisabled={item.listingStatus !== "withdrawn"}
                              onAction={() => setDeleting(item)}
                            >
                              {item.listingStatus === "withdrawn"
                                ? "删除"
                                : "删除（请先下架）"}
                            </Dropdown.Item>
                          </Dropdown.Menu>
                        </Dropdown.Popover>
                      </Dropdown>
                    </ButtonGroup>
                  </Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Content>
        </Table.ScrollContainer>
        <Table.Footer>共 {data.length} 个商品</Table.Footer>
      </Table>
      {submissionsOpen ? (
        <ApplicationSubmissionsModal
          customerNames={customerNames}
          customersLoading={customersLoading}
          customersError={customersError}
          onReviewed={refresh}
          onClose={() => setSubmissionsOpen(false)}
        />
      ) : null}
      {versionProduct ? (
        <ModalShell
          title={`选择 ${versionProduct.name} 的上架版本`}
          onClose={() => setVersionProduct(null)}
        >
          <p className="mb-3 text-sm text-muted">
            上架时采用该发布版本最新通过审核的商业配置，既有订单价格不变。
          </p>
          <div className="space-y-2">
            {releases.map((release) => (
              <Button
                isDisabled={mutation.pending}
                key={release.submissionId}
                className="w-full justify-between"
                variant="secondary"
                onPress={() => void listVersion(release.submissionId)}
              >
                v{release.version}
                <span className="text-xs">
                  {formatDate(release.submittedAt)}
                </span>
              </Button>
            ))}
            {releases.length === 0 ? (
              <p className="text-sm text-muted">暂无可上架的已审核版本。</p>
            ) : null}
          </div>
        </ModalShell>
      ) : null}
      {deleting ? (
        <ApplicationDeleteModal
          product={deleting}
          onClose={() => setDeleting(null)}
          onDeleted={refresh}
        />
      ) : null}
      {selected ? (
        <ApplicationGiftModal
          product={selected}
          onClose={() => setSelected(null)}
          onGranted={refresh}
        />
      ) : null}
    </>
  );
}
