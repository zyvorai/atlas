// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

type FeatureItem = {
  eyebrow: string;
  title: string;
  description: ReactNode;
  to: string;
  cta: string;
};

const FeatureList: FeatureItem[] = [
  {
    eyebrow: 'Control plane',
    title: 'Intent → storage',
    description:
      'Volumes, snapshots, clones, CephFS RWX, and RGW buckets through stable REST and gRPC — products never speak Ceph directly.',
    to: '/docs/core-concepts/architecture',
    cta: 'Architecture',
  },
  {
    eyebrow: 'Drivers',
    title: 'One trait. Three backends.',
    description:
      'Real Ceph first, plus NFS and ZFS behind StorageDriver — and a fake driver for zero-cluster local demo.',
    to: '/docs/core-concepts/architecture',
    cta: 'How drivers work',
  },
  {
    eyebrow: 'DataBridge',
    title: 'Cloud to edge, on Ceph.',
    description:
      'Six source engines, CDC, cutover, and Ceph-backed edge targets — migration as a control-plane product.',
    to: '/docs/getting-started/quickstart',
    cta: 'Quickstart',
  },
  {
    eyebrow: 'Day-2',
    title: 'Operate without leaving the shop.',
    description:
      'Alerts, maintenance, governance, quotas, orphan GC, upgrade preflight, and DR scaffolding in one console.',
    to: '/gallery',
    cta: 'See the console',
  },
];

function Feature({eyebrow, title, description, to, cta}: FeatureItem) {
  return (
    <article className={styles.band}>
      <div className={styles.inner}>
        <p className={styles.eyebrow}>{eyebrow}</p>
        <Heading as="h2" className={styles.title}>
          {title}
        </Heading>
        <p className={styles.copy}>{description}</p>
        <Link className={styles.link} to={to}>
          {cta} →
        </Link>
      </div>
    </article>
  );
}

export default function FeatureHighlights(): ReactNode {
  return (
    <section className={styles.features} aria-label="Capabilities">
      {FeatureList.map((props) => (
        <Feature key={props.title} {...props} />
      ))}
    </section>
  );
}
