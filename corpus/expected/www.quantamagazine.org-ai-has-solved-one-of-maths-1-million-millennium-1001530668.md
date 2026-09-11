# AI Has Solved One of Math’s $1 Million Millennium Prize Problems | Quanta Magazine

By Konstantin Kakaes September 8, 2026

On the morning of Tuesday, September 8, mathematicians at OpenAI announced that a group of 10,000 autonomous AI agents under their direction, running on an advanced model not available to the public, had found a “singularity” in the Navier-Stokes equations in three dimensions — thus resolving [one of the six remaining](https://www.claymath.org/millennium/navier-stokes-equation/) Millennium Prize Problems posed in 2000 by the Clay Mathematics Institute, each of which carries a $1 million prize. Their result has been formally checked in the programming language Lean, giving mathematicians confidence that it is indeed correct.

If the result holds up to further scrutiny, it is, by a significant margin, the most important mathematical proof to have been arrived at by an artificial-intelligence model to date, possibly marking a fundamental turning point in how mathematicians tackle difficult problems.

This particular difficult problem deals with differential equations, which express relationships between changing quantities. They are arguably the single most important mathematical tool for explaining the world around us. As a rule, they are easy to write down and hard to solve.

The Navier-Stokes equations are differential equations that use Newton’s second law of motion to describe how fluids, from ocean currents to air flows, behave. They were first written down in the mid-19th century, and have been central to the study of fluid mechanics ever since. But one basic question about the equations has persisted: Are their solutions always well-behaved? Or can their solutions evolve over time so that some infinitesimally small part of the fluid begins to flow infinitely quickly, creating a so-called singularity?

The OpenAI announcement of this long-sought singularity came 12 hours after an announcement from [Tristan Buckmaster](https://cims.nyu.edu/~tristanb/) at New York University that he, together with Levent Alpöge at Anthropic, had [resolved several closely related problems](https://mastodon.social/@tristanbuckmaster/117233413705701198) with help from a variety of AI models, including those of OpenAI.

Both AI-enabled teams relied heavily on work by [Diego Córdoba](https://www.icmat.es/people/dcordoba/) of the Institute for Mathematical Sciences in Madrid and [Luis Martínez-Zoroa](https://www.cunef.edu/en/faculty-and-research/martinez-zoroa-luis/) of CUNEF University, researchers who had developed a strategy to attack the problem that radically departed from the methods most mathematicians were using.

“I was thrilled that the problem was solved,” said [Charles Fefferman](https://www.math.princeton.edu/people/charles-fefferman) of Princeton University, who wrote the Clay Institute’s [official description](https://www.claymath.org/wp-content/uploads/2022/06/navierstokes.pdf) of the Navier-Stokes problem. The heroes of the story, he said, are Córdoba and Martínez-Zoroa. As Buckmaster wrote in a statement announcing his results, “Let me make plain what I have said to colleagues in private: in view of this body of work, I believe Luis Martínez-Zoroa deserves a Fields Medal.”

## **One Singular Sensation**

The Navier-Stokes equations rely on the assumption that you can zoom in on a fluid, considering endlessly smaller amounts of it. The real world is not like this: Fluids are ultimately made of molecules and atoms. They are not perfectly smooth. This means that the mathematical results about the formation of singularities don’t have any immediate practical consequences. However, those results are important because it’s surprising that such singularities are possible even in an idealized sense. It tells us that, straightforward as Newton’s second law appears to be, its consequences when applied to fluids are profoundly counterintuitive. Put another way: Turbulence is even weirder than it appears to be.

The Navier-Stokes equations account for the fact that fluids can have viscosity, or friction. (Fluids with more viscosity, like honey, flow slowly, while those with less viscosity, like water, flow more quickly.) A simpler, related set of equations called the Euler equations describe fluids with zero viscosity, which flow without friction. The two sets of equations are closely related — researchers often work in parallel on both. But introducing even an infinitesimal amount of friction causes a fluid to behave in a profoundly different way.

![](https://www.quantamagazine.org/wp-content/uploads/2026/09/navier-stokes-light-master.webp)

OpenAI’s solution is a vortex, visualized here, where yellow represents a faster rotation speed and blue slower.

OpenAI

“Ten years ago, nobody believed there was a singularity for Navier-Stokes,” said Córdoba — though many believed that the Euler equations did admit a singularity. This began to change in 2013, when [Thomas Hou](https://www.cms.caltech.edu/people/hou) of the California Institute of Technology and [Guo Luo](https://msi.hsu.edu.hk/en/staff/dr-luo-guo/), now at the Hang Seng University of Hong Kong, derived a groundbreaking result showing that the Euler equations [can “blow up,”](https://www.quantamagazine.org/for-fluid-equations-a-steady-flow-of-progress-20200113/) as mathematicians like to say, in a cylinder if the top and bottom halves are set spinning in opposite directions. “That’s the first really serious claim of singularity,” Córdoba remembered. Over the next few years, [a series of results](https://www.quantamagazine.org/famous-fluid-equations-spring-a-leak-20191218/) including a [2019 paper](https://arxiv.org/abs/1904.04795) got mathematicians thinking that not only might the Euler equations have singularities, but that Navier-Stokes might as well.

There are a few intellectual steps to get from there to the present day. The first is the question of a boundary. The Millennium Prize version of the problem asks what happens in three-dimensional space that extends indefinitely in all directions. Earlier results, like the 2013 cylinder one, posit the existence of some boundary. These are useful intermediate findings, but the case without a boundary is mathematically interesting, according to Fefferman, because in models with a boundary, finding a singularity “tells you the fluid can form a singularity thanks to its interaction with the boundary — but without a boundary it’s the fluid doing the crazy stuff.”

The next major step involves modeling the forces that cause fluids to move. These could be something as natural as gravity pulling the fluid downwards, or an artificial intervention like a propeller. Mathematicians model these forces with a technique called “forcing.” They’ve sometimes tried to introduce awkward, ungainly forcing functions to get fluids to behave in odd ways. But the Millennium Prize version of the problem asks what can happen when the forcing function is mathematically well-behaved, or “smooth.”

In their 2013 cylinder result, Hou and Luo used computer models to simulate a scenario that might lead the Euler equations to blow up. They then proved that blowup does occur by computationally accounting for all potential errors. In the years since, similar techniques have become the dominant mode of attack on the Euler and Navier-Stokes problems.

But in his 2021 doctoral dissertation, Martínez-Zoroa pioneered analytic techniques that don’t rely on computers at all. This made him, along with Córdoba, his doctoral adviser, an oddball in the area. By 2023, the pair had [proved that a version](https://arxiv.org/abs/2309.08495) of the Euler equations with a messy forcing function displayed singularities.

Córdoba likes to joke: “I don’t use AI: I have Luis.” Martínez-Zoroa added that it’s not that either of them is opposed to AI, but that “up to relatively recently, when I tried to use it for my own work, it didn’t match very well with my workflow. I’ll have to adapt, clearly.”

In broad outline, the pair’s technique relies on creating an infinite sequence of “layers,” each of which is a non-singular solution to the equation they are studying. (They’ve applied similar techniques to both the Euler and Navier-Stokes equations, as well as to other related systems.) They then combine those solutions in what Martínez-Zoroa calls an “infinite cascade” to produce a new solution.

That new solution, they showed, contains the desired singularity. However, even though each individual layer relies on a smooth forcing function, combining them together can cause the forcing function to have undesirable mathematical properties. That’s why their solution fell short of satisfying the Millennium Prize criteria. The remaining hurdle was to figure out how to create a similar infinite cascade that resulted not only in a singularity, but also in a smooth forcing function.

That’s the step that both competing AI groups appear to have had success with.

## **One Thrilling Combination**

As Córdoba and Martínez-Zoroa were working on their techniques, AI-derived math research has been on something of a binge. Since the beginning of the summer, large language models, primarily from OpenAI and Anthropic, have been used to obtain proofs of results across many areas of math. Often these results have been accompanied by formal proofs, which rely on the programming language Lean to establish, with airtight certainty, that a statement must be true. (The crucial bit of verification that must still be done by humans is to guarantee that the statement being shown to be true in Lean is logically equivalent to what mathematicians set out to prove.)

For much of the summer, that competition has seemed, if not friendly, at least not openly acrimonious. That changed in the last 24 hours.

Buckmaster [shared a statement](https://cims.nyu.edu/~tristanb/statement.pdf) just before midnight on Monday, September 7, announcing the results of his collaboration with Alpöge. “For most of the past year progress was slow,” Buckmaster said in his statement. But by August 22, they had a Lean-verified proof for the Euler equations: “I can say the first LLM generated proof Levent sent me was the most horrendous I have ever read,” Buckmaster wrote.

The duo had planned to continue working on a more elegant write-up, but after word of their progress leaked to OpenAI, they felt they had to move up their timeline. Buckmaster lamented that one of the three papers he and Alpöge were releasing “can only be described as AI slop. I am sorry for this.”

OpenAI admits that their work on the Navier-Stokes equation was inspired by rumors that Alpöge and Buckmaster had solved a Millennium Prize problem. (They hadn’t quite, though they say they have an unverified proof of blowup for a somewhat easier version of Navier-Stokes.) Using a new internal model, OpenAI employed groups of autonomous AI agents of various sizes to attack variants of the Euler and Navier-Stokes problems. “Nearly 100 agents worked together for approximately 50 hours to produce our Euler regularity disproof,” the company wrote in a press release. They then deployed an even bigger group of agents — 10,000 or so — to attack Navier-Stokes. After 88 hours, the agents running on the internal model had a proof of a singularity in Navier-Stokes, and after an additional 17 hours, another AI model had formalized the result. In total, the agents sent almost 5 million messages to each other. Sébastien Bubeck of OpenAI estimates the computational cost at several million dollars.

As this story went to press, the details of the interaction between Buckmaster, Alpöge, and OpenAI remain murky — different parties to the conversation are presenting different versions. OpenAI cedes priority for the 3D Euler result to Buckmaster and Alpöge, while claiming it for the Navier-Stokes result. In Buckmaster’s statement, he appears to suggest that the OpenAI researchers or their AI agents may have gained access to (and benefited from) the work that he and Alpöge did using OpenAI’s models.

It will take some time to sort out the timelines of who did what when and to understand the mathematical significance of the new AI proofs, even if they have already been formally verified. But regardless, the intellectual debt to Córdoba and Martínez-Zoroa seems clear. “I’m very happy for Tristan,” Martínez-Zoroa said. “It would have been nice to do this ourselves, but I’m very happy for him.”
