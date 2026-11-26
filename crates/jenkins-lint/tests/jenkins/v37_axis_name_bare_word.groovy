pipeline {
    agent none
    stages {
        stage('M') {
            matrix {
                agent any
                axes {
                    axis {
                        name compiler
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
