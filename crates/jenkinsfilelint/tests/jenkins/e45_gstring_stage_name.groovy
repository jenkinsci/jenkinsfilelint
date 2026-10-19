def NAME = 'x'
pipeline {
    agent any
    stages {
        stage("Build ${NAME}") { steps { echo 'x' } }
    }
}
