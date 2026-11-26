pipeline {
    agent none
    stages {
        stage('M') {
            matrix {
                agent any
                axes {
                    axis {
                        name 42
                        values 'gcc', 'clang'
                    }
                }
                stages {
                    stage('B') { steps { echo 'x' } }
                }
            }
        }
    }
}
