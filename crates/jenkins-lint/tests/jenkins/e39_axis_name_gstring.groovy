pipeline {
    agent none
    stages {
        stage('M') {
            matrix {
                agent any
                axes {
                    axis {
                        name "${env.X}"
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
