import type {ReactNode} from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  //Svg: React.ComponentType<React.ComponentProps<'svg'>>;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Privacy Primitives',
    //Svg: require('@site/static/img/undraw_docusaurus_mountain.svg').default,
    description: (
      <>
        Shielded notes, nullifiers, encrypted outputs, and tx blobs live in
        <code>sdk/privacy</code> and <code>sdk/txblob</code>, ready for wallet
        and prover integrations.
      </>
    ),
  },
  {
    title: 'Sequencer + Prover Pipeline',
    //Svg: require('@site/static/img/undraw_docusaurus_tree.svg').default,
    description: (
      <>
        The <code>core/</code> sequencer batches transactions, updates state,
        and settles to Solana, with Groth16 proving support in
        <code>prover/</code>.
      </>
    ),
  },
  {
    title: 'Solana Programs + SDKs',
    //Svg: require('@site/static/img/undraw_docusaurus_react.svg').default,
    description: (
      <>
        On-chain bridge/verifier programs live in
        <code>onchain-programs/</code>, with Rust and TypeScript SDKs under
        <code>sdk/</code>.
      </>
    ),
  },
];

type FeatureProps = FeatureItem & {index: number};

function Feature({title, /*Svg,*/ description, index}: FeatureProps) {
  return (
    <div
      className={clsx('col col--4', styles.featureCard)}
      style={{animationDelay: `${index * 120}ms`}}>
      <div className="text--center">
        {/* <Svg className={styles.featureSvg} role="img" /> */}
      </div>
      <div className="text--center padding-horiz--md">
        <Heading as="h3" className={styles.featureTitle}>
          {title}
        </Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} index={idx} />
          ))}
        </div>
      </div>
    </section>
  );
}
