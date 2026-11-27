pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                axes {
                    axis {
                        name 'OS'
                        values 'linux', 'mac'
                    }
                }
                stages {
                    failFast true
                    stage('T') { steps { echo 'x' } }
                }
            }
        }
    }
}
