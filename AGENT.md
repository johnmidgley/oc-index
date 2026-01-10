These instructions are for any agent, human or otherwise, working on the repository.

The README.md file serves as both specification and user documentation for the project. Coding should start from this specification. Over the lifecycle of the project, any changes to it should be refected in the code and any code changes that merit explicit specification or user level documentation should be added to it.   

All code written shall be well factored for testability and reusability. In particular, functions should generally do one thing. Mutability should be avoided whenever possible, unless a performance is a higher priority. All code should be written in a form idomatic to the target language.

Unit and / or integration tests should be written to ensure reliable functioning as the codebase evolves. 

dev_log.md contains any information that would be useful for a human to know about the project. It can be updated with design decisions, external references, or anything that is useful for the project outside the codebase that is not suitable as specification or user documentation.