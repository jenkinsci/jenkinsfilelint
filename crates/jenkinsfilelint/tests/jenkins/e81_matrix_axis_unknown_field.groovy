pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                axes {
                    axis {
                        name 'OS'
                        values 'linux'
                        labels 'x'
                    }
                }
                stages {
                    stage('S') {
                        steps { echo 'x' }
                    }
                }
            }
        }
    }
}
