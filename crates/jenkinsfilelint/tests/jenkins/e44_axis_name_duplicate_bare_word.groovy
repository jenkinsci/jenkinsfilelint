pipeline {
    agent none
    stages {
        stage('M') {
            matrix {
                agent any
                axes {
                    axis {
                        name compiler
                        values 'gcc'
                    }
                    axis {
                        name 'compiler'
                        values 'x'
                    }
                }
                stages {
                    stage('B') { steps { echo 'x' } }
                }
            }
        }
    }
}
